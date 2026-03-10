use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use arc_llm::provider::Provider;
use dialoguer::{Confirm, Input, MultiSelect};
use rand::Rng;

use crate::doctor;

// ---------------------------------------------------------------------------
// OpenSSL helpers
// ---------------------------------------------------------------------------

/// Run an openssl subcommand and return stdout on success.
fn run_openssl(args: &[&str], description: &str) -> Result<Vec<u8>> {
    let output = Command::new("openssl")
        .args(args)
        .output()
        .with_context(|| format!("failed to run openssl for: {description}"))?;
    if !output.status.success() {
        bail!(
            "openssl {description} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(output.stdout)
}

/// Run an openssl subcommand that reads key material from stdin.
fn run_openssl_with_stdin(args: &[&str], stdin_data: &[u8], description: &str) -> Result<Vec<u8>> {
    let mut child = Command::new("openssl")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn openssl for: {description}"))?;
    child
        .stdin
        .take()
        .context("openssl process missing stdin")?
        .write_all(stdin_data)
        .with_context(|| format!("failed to write to openssl stdin for: {description}"))?;
    let output = child
        .wait_with_output()
        .with_context(|| format!("failed to read openssl output for: {description}"))?;
    if !output.status.success() {
        bail!(
            "openssl {description} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(output.stdout)
}

// ---------------------------------------------------------------------------
// Session secret
// ---------------------------------------------------------------------------

fn generate_session_secret() -> String {
    let mut rng = rand::thread_rng();
    let bytes: [u8; 32] = rng.gen();
    hex::encode(&bytes)
}

// ---------------------------------------------------------------------------
// JWT keypair generation
// ---------------------------------------------------------------------------

fn generate_jwt_keypair() -> Result<(String, String)> {
    let private_pem = run_openssl(&["genpkey", "-algorithm", "Ed25519"], "generate keypair")?;
    let public_pem =
        run_openssl_with_stdin(&["pkey", "-pubout"], &private_pem, "extract public key")?;

    let private_str = String::from_utf8(private_pem).context("private key is not valid UTF-8")?;
    let public_str = String::from_utf8(public_pem).context("public key is not valid UTF-8")?;
    Ok((private_str, public_str))
}

// ---------------------------------------------------------------------------
// mTLS certificate generation
// ---------------------------------------------------------------------------

fn generate_mtls_certs(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir).context("failed to create certs directory")?;

    // 1. CA key + self-signed cert
    let ca_key = run_openssl(&["genpkey", "-algorithm", "Ed25519"], "generate CA key")?;
    let ca_key_path = dir.join("ca.key");
    std::fs::write(&ca_key_path, &ca_key)?;

    let ca_cert = run_openssl(
        &[
            "req",
            "-new",
            "-x509",
            "-key",
            ca_key_path
                .to_str()
                .context("CA key path is not valid UTF-8")?,
            "-days",
            "3650",
            "-subj",
            "/CN=Arc CA",
        ],
        "generate CA cert",
    )?;
    let ca_cert_path = dir.join("ca.crt");
    std::fs::write(&ca_cert_path, &ca_cert)?;

    // 2. Server key + CSR signed by CA
    let server_key = run_openssl(&["genpkey", "-algorithm", "Ed25519"], "generate server key")?;
    let server_key_path = dir.join("server.key");
    std::fs::write(&server_key_path, &server_key)?;

    let csr = run_openssl_with_stdin(
        &[
            "req",
            "-new",
            "-key",
            "/dev/stdin",
            "-subj",
            "/CN=localhost",
        ],
        &server_key,
        "generate server CSR",
    )?;

    let csr_path = dir.join("server.csr");
    std::fs::write(&csr_path, &csr)?;

    let server_cert = run_openssl(
        &[
            "x509",
            "-req",
            "-in",
            csr_path.to_str().context("CSR path is not valid UTF-8")?,
            "-CA",
            ca_cert_path
                .to_str()
                .context("CA cert path is not valid UTF-8")?,
            "-CAkey",
            ca_key_path
                .to_str()
                .context("CA key path is not valid UTF-8")?,
            "-CAcreateserial",
            "-days",
            "3650",
        ],
        "sign server cert",
    )?;
    std::fs::write(dir.join("server.crt"), &server_cert)?;

    // Clean up temporary files
    let _ = std::fs::remove_file(&csr_path);
    let _ = std::fs::remove_file(dir.join("ca.srl"));

    Ok(())
}

// ---------------------------------------------------------------------------
// Config TOML generation
// ---------------------------------------------------------------------------

fn format_config_toml(username: &str) -> String {
    format!(
        r#"[web]
url = "http://localhost:5173"

[web.auth]
provider = "github"
allowed_usernames = ["{username}"]

[api]
base_url = "https://localhost:3000"
authentication_strategies = ["jwt", "mtls"]

[api.tls]
cert = "~/.arc/certs/server.crt"
key = "~/.arc/certs/server.key"
ca = "~/.arc/certs/ca.crt"
"#
    )
}

// ---------------------------------------------------------------------------
// .env merge
// ---------------------------------------------------------------------------

fn merge_env(existing: &str, new_vars: &[(&str, &str)]) -> String {
    let mut result_lines: Vec<String> = Vec::new();
    let mut handled_keys: std::collections::HashSet<&str> = std::collections::HashSet::new();

    for line in existing.lines() {
        if let Some(eq_pos) = line.find('=') {
            let key = line[..eq_pos].trim();
            if !key.is_empty() && !key.starts_with('#') {
                if let Some((_, new_val)) = new_vars.iter().find(|(k, _)| *k == key) {
                    result_lines.push(format!("{key}={new_val}"));
                    handled_keys.insert(key);
                    continue;
                }
            }
        }
        result_lines.push(line.to_string());
    }

    for (key, val) in new_vars {
        if !handled_keys.contains(*key) {
            result_lines.push(format!("{key}={val}"));
        }
    }

    let mut result = result_lines.join("\n");
    if !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

// ---------------------------------------------------------------------------
// Provider key URLs
// ---------------------------------------------------------------------------

fn provider_key_url(provider: Provider) -> &'static str {
    match provider {
        Provider::Anthropic => "https://console.anthropic.com/settings/keys",
        Provider::OpenAi => "https://platform.openai.com/api-keys",
        Provider::Gemini => "https://aistudio.google.com/apikey",
        Provider::Kimi => "https://platform.moonshot.cn/console/api-keys",
        Provider::Zai => "https://open.bigmodel.cn/usercenter/apikeys",
        Provider::Minimax => {
            "https://platform.minimaxi.com/user-center/basic-information/interface-key"
        }
        Provider::Inception => "https://console.inceptionlabs.ai/api-keys",
    }
}

fn provider_display_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Anthropic => "Anthropic",
        Provider::OpenAi => "OpenAI",
        Provider::Gemini => "Gemini",
        Provider::Kimi => "Kimi",
        Provider::Zai => "Zai",
        Provider::Minimax => "Minimax",
        Provider::Inception => "Inception",
    }
}

// ---------------------------------------------------------------------------
// Interactive setup
// ---------------------------------------------------------------------------

fn prompt_confirm(prompt: &str, default: bool) -> Result<bool> {
    Ok(
        Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
            .with_prompt(prompt)
            .default(default)
            .interact_on(&dialoguer::console::Term::stderr())?,
    )
}

fn prompt_input(prompt: &str) -> Result<String> {
    Ok(
        Input::with_theme(&dialoguer::theme::ColorfulTheme::default())
            .with_prompt(prompt)
            .interact_on(&dialoguer::console::Term::stderr())?,
    )
}

fn prompt_multiselect(prompt: &str, items: &[String]) -> Result<Vec<usize>> {
    Ok(
        MultiSelect::with_theme(&dialoguer::theme::ColorfulTheme::default())
            .with_prompt(prompt)
            .items(items)
            .interact_on(&dialoguer::console::Term::stderr())?,
    )
}

pub async fn run_install() -> Result<()> {
    eprintln!("Arc Install");
    eprintln!("===========");
    eprintln!();

    let arc_dir = dirs::home_dir()
        .context("could not determine home directory")?
        .join(".arc");
    std::fs::create_dir_all(&arc_dir)?;

    // Step 0: Pre-flight checks
    eprintln!("[Step 0/7] Pre-flight checks");
    let dep_outcomes = doctor::probe_system_deps();
    let dep_check = doctor::check_system_deps(doctor::DEP_SPECS, &dep_outcomes);

    if dep_check.status == doctor::CheckStatus::Error {
        eprintln!("  Missing required system dependencies:");
        for detail in &dep_check.details {
            eprintln!("    {}", detail.text);
        }
        bail!("Install missing required tools before running setup");
    }

    // Check if dot is missing and offer to install
    let dot_idx = doctor::DEP_SPECS.iter().position(|s| s.name == "dot");
    if let Some(idx) = dot_idx {
        if matches!(dep_outcomes[idx], doctor::ProbeOutcome::NotFound) {
            let install = tokio::task::spawn_blocking(|| {
                prompt_confirm("Graphviz (dot) not found. Install via Homebrew?", true)
            })
            .await??;

            if install {
                let status = Command::new("brew")
                    .args(["install", "graphviz"])
                    .status()
                    .context("failed to run brew install graphviz")?;
                if !status.success() {
                    eprintln!("  Warning: brew install graphviz failed");
                }
            }
        }
    }

    for detail in &dep_check.details {
        eprintln!("  {}", detail.text);
    }
    eprintln!();

    // Step 1: Configuration
    eprintln!("[Step 1/7] Configuration");
    let config_path = arc_dir.join("server.toml");
    let write_config = if config_path.exists() {
        tokio::task::spawn_blocking(|| {
            prompt_confirm("~/.arc/server.toml already exists. Overwrite?", false)
        })
        .await??
    } else {
        true
    };

    if write_config {
        let username: String =
            tokio::task::spawn_blocking(|| prompt_input("GitHub username for allowed access"))
                .await??;

        let toml_content = format_config_toml(&username);
        std::fs::write(&config_path, &toml_content)?;
        eprintln!("  Wrote {}", config_path.display());
    } else {
        eprintln!("  Keeping existing server.toml");
    }
    eprintln!();

    // Step 2: Generating secrets and certificates
    eprintln!("[Step 2/7] Generating secrets and certificates");

    let session_secret = generate_session_secret();
    eprintln!("  [ok] Session secret generated");

    let (jwt_private_pem, jwt_public_pem) = generate_jwt_keypair()?;
    eprintln!("  [ok] Ed25519 JWT keypair generated");

    let certs_dir = arc_dir.join("certs");
    generate_mtls_certs(&certs_dir)?;
    eprintln!("  [ok] mTLS CA + server certificates generated");
    eprintln!();

    // Step 3: LLM providers
    eprintln!("[Step 3/7] LLM providers");
    let provider_labels: Vec<String> = Provider::ALL
        .iter()
        .map(|p| {
            let env_vars = p.api_key_env_vars().join(" / ");
            format!("{} ({})", provider_display_name(*p), env_vars)
        })
        .collect();

    let selected_indices: Vec<usize> = tokio::task::spawn_blocking({
        let labels = provider_labels.clone();
        move || prompt_multiselect("Which LLM providers do you want to configure?", &labels)
    })
    .await??;

    let mut env_pairs: Vec<(String, String)> = Vec::new();
    for idx in selected_indices {
        let provider = Provider::ALL[idx];
        let env_var = provider.api_key_env_vars()[0];
        let url = provider_key_url(provider);
        eprintln!("  Get your API key at: {url}");

        let prompt = env_var.to_string();
        let key: String = tokio::task::spawn_blocking(move || prompt_input(&prompt)).await??;

        env_pairs.push((env_var.to_string(), key));
    }
    eprintln!();

    // Step 4: Writing ~/.arc/.env
    eprintln!("[Step 4/7] Writing ~/.arc/.env");
    let env_path = arc_dir.join(".env");

    let jwt_private_b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        jwt_private_pem.as_bytes(),
    );
    let jwt_public_b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        jwt_public_pem.as_bytes(),
    );

    env_pairs.push(("ARC_JWT_PRIVATE_KEY".to_string(), jwt_private_b64));
    env_pairs.push(("ARC_JWT_PUBLIC_KEY".to_string(), jwt_public_b64));
    env_pairs.push(("SESSION_SECRET".to_string(), session_secret));

    let existing_env = std::fs::read_to_string(&env_path).unwrap_or_default();
    let env_refs: Vec<(&str, &str)> = env_pairs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let merged = merge_env(&existing_env, &env_refs);
    std::fs::write(&env_path, &merged)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&env_path, std::fs::Permissions::from_mode(0o600))?;
    }

    eprintln!(
        "  Wrote {} ({} variables)",
        env_path.display(),
        env_pairs.len()
    );
    eprintln!();

    // Step 5: Start servers
    eprintln!("[Step 5/7] Start servers");
    eprintln!("  To start Arc, run these commands:");
    eprintln!();
    eprintln!("    arc serve");
    eprintln!("    cd apps/arc-web && npx react-router dev");
    eprintln!();

    // Step 6: Verify setup
    eprintln!("[Step 6/7] Verify setup");
    let run_doctor =
        tokio::task::spawn_blocking(|| prompt_confirm("Run arc doctor to verify?", true)).await??;

    if run_doctor {
        eprintln!();
        doctor::run_doctor(true, false).await;
    }

    eprintln!();
    eprintln!("Setup complete!");
    Ok(())
}

// ---------------------------------------------------------------------------
// Hex encoding (avoid adding a dep just for this)
// ---------------------------------------------------------------------------

mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    // -- Session secret --

    #[test]
    fn session_secret_length() {
        let secret = generate_session_secret();
        assert_eq!(secret.len(), 64);
    }

    #[test]
    fn session_secret_is_hex() {
        let secret = generate_session_secret();
        assert!(secret.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn session_secret_is_lowercase() {
        let secret = generate_session_secret();
        assert!(secret.chars().all(|c| !c.is_ascii_uppercase()));
    }

    // -- JWT keypair --

    #[test]
    fn jwt_keypair_private_pem_header() {
        let (private, _) = generate_jwt_keypair().unwrap();
        assert!(
            private.starts_with("-----BEGIN PRIVATE KEY-----"),
            "private PEM: {private}"
        );
    }

    #[test]
    fn jwt_keypair_public_pem_header() {
        let (_, public) = generate_jwt_keypair().unwrap();
        assert!(
            public.starts_with("-----BEGIN PUBLIC KEY-----"),
            "public PEM: {public}"
        );
    }

    #[test]
    fn jwt_keypair_public_parses() {
        let (_, public) = generate_jwt_keypair().unwrap();
        jsonwebtoken::DecodingKey::from_ed_pem(public.as_bytes()).expect("public key should parse");
    }

    // -- mTLS cert generation --

    #[test]
    fn mtls_certs_creates_files() {
        let dir = tempfile::tempdir().unwrap();
        let certs_dir = dir.path().join("certs");
        generate_mtls_certs(&certs_dir).unwrap();

        assert!(certs_dir.join("ca.key").exists());
        assert!(certs_dir.join("ca.crt").exists());
        assert!(certs_dir.join("server.key").exists());
        assert!(certs_dir.join("server.crt").exists());
    }

    #[test]
    fn mtls_ca_cert_is_pem() {
        let dir = tempfile::tempdir().unwrap();
        let certs_dir = dir.path().join("certs");
        generate_mtls_certs(&certs_dir).unwrap();

        let ca_crt = std::fs::read_to_string(certs_dir.join("ca.crt")).unwrap();
        assert!(
            ca_crt.starts_with("-----BEGIN CERTIFICATE-----"),
            "ca.crt: {ca_crt}"
        );
    }

    #[test]
    fn mtls_server_cert_is_pem() {
        let dir = tempfile::tempdir().unwrap();
        let certs_dir = dir.path().join("certs");
        generate_mtls_certs(&certs_dir).unwrap();

        let server_crt = std::fs::read_to_string(certs_dir.join("server.crt")).unwrap();
        assert!(
            server_crt.starts_with("-----BEGIN CERTIFICATE-----"),
            "server.crt: {server_crt}"
        );
    }

    #[test]
    fn mtls_certs_parse_via_rustls() {
        let dir = tempfile::tempdir().unwrap();
        let certs_dir = dir.path().join("certs");
        generate_mtls_certs(&certs_dir).unwrap();

        let ca_pem = std::fs::read(certs_dir.join("ca.crt")).unwrap();
        let mut reader = std::io::Cursor::new(&ca_pem);
        let ca_certs: Vec<_> = rustls_pemfile::certs(&mut reader)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(ca_certs.len(), 1);

        let server_pem = std::fs::read(certs_dir.join("server.crt")).unwrap();
        let mut reader = std::io::Cursor::new(&server_pem);
        let server_certs: Vec<_> = rustls_pemfile::certs(&mut reader)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(server_certs.len(), 1);
    }

    // -- Config TOML generation --

    #[test]
    fn config_toml_roundtrips() {
        let toml_str = format_config_toml("brynary");
        let config: arc_config::server::ServerConfig =
            toml::from_str(&toml_str).expect("config should parse");
        assert_eq!(config.web.auth.allowed_usernames, vec!["brynary"]);
    }

    #[test]
    fn config_toml_has_auth_strategies() {
        let toml_str = format_config_toml("alice");
        let config: arc_config::server::ServerConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(
            config.api.authentication_strategies,
            vec![
                arc_config::server::ApiAuthStrategy::Jwt,
                arc_config::server::ApiAuthStrategy::Mtls,
            ]
        );
    }

    #[test]
    fn config_toml_has_tls_paths() {
        let toml_str = format_config_toml("bob");
        let config: arc_config::server::ServerConfig = toml::from_str(&toml_str).unwrap();
        let tls = config.api.tls.expect("tls should be set");
        assert_eq!(tls.cert, PathBuf::from("~/.arc/certs/server.crt"));
        assert_eq!(tls.key, PathBuf::from("~/.arc/certs/server.key"));
        assert_eq!(tls.ca, PathBuf::from("~/.arc/certs/ca.crt"));
    }

    // -- .env merge --

    #[test]
    fn merge_env_replaces_existing() {
        let result = merge_env("FOO=old\nBAR=keep\n", &[("FOO", "new"), ("BAZ", "added")]);
        assert!(result.contains("FOO=new"));
        assert!(result.contains("BAR=keep"));
        assert!(result.contains("BAZ=added"));
    }

    #[test]
    fn merge_env_empty_existing() {
        let result = merge_env("", &[("FOO", "bar"), ("BAZ", "qux")]);
        assert!(result.contains("FOO=bar"));
        assert!(result.contains("BAZ=qux"));
    }

    #[test]
    fn merge_env_preserves_comments_and_blanks() {
        let existing = "# A comment\n\nFOO=old\n# Another\nBAR=keep\n";
        let result = merge_env(existing, &[("FOO", "new")]);
        assert!(result.contains("# A comment"));
        assert!(result.contains("# Another"));
        assert!(result.contains("FOO=new"));
        assert!(result.contains("BAR=keep"));
    }

    #[test]
    fn merge_env_full_scenario() {
        let result = merge_env("FOO=old\nBAR=keep", &[("FOO", "new"), ("BAZ", "added")]);
        assert_eq!(result, "FOO=new\nBAR=keep\nBAZ=added\n");
    }

    // -- Provider key URLs --

    #[test]
    fn every_provider_has_key_url() {
        for provider in Provider::ALL {
            let url = provider_key_url(*provider);
            assert!(!url.is_empty(), "{provider:?} has empty URL");
            assert!(url.starts_with("https://"), "{provider:?} URL: {url}");
        }
    }
}
