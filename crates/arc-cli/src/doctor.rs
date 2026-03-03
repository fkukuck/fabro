use std::fmt::Write;
use std::path::PathBuf;

use arc_api::server_config::{ApiAuthenticationStrategy, AuthProvider};
use arc_llm::provider::Provider;
use arc_util::terminal::Styles;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Pass,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct CheckDetail {
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct CheckResult {
    pub name: String,
    pub status: CheckStatus,
    pub summary: String,
    pub details: Vec<CheckDetail>,
    pub remediation: Option<String>,
}

pub struct DoctorReport {
    pub checks: Vec<CheckResult>,
}

impl DoctorReport {
    pub fn has_errors(&self) -> bool {
        self.checks
            .iter()
            .any(|c| c.status == CheckStatus::Error)
    }

    pub fn issue_count(&self) -> usize {
        self.checks
            .iter()
            .filter(|c| matches!(c.status, CheckStatus::Warning | CheckStatus::Error))
            .count()
    }

    pub fn render(&self, s: &Styles, verbose: bool) -> String {
        let mut out = String::new();

        writeln!(out, "{b}Arc Doctor{r}", b = s.bold, r = s.reset).unwrap();
        writeln!(out).unwrap();

        for check in &self.checks {
            let (icon, color) = match check.status {
                CheckStatus::Pass => ("[✓]", s.green),
                CheckStatus::Warning => ("[!]", s.yellow),
                CheckStatus::Error => ("[✗]", s.red),
            };

            writeln!(
                out,
                "  {color}{icon}{r} {b}{name}{r} ({summary})",
                color = color,
                r = s.reset,
                b = s.bold,
                name = check.name,
                summary = check.summary,
            )
            .unwrap();

            if verbose {
                for detail in &check.details {
                    writeln!(out, "      • {}", detail.text).unwrap();
                }
            }
        }

        let issues = self.issue_count();
        writeln!(out).unwrap();

        if issues == 0 {
            writeln!(out, "All checks passed.").unwrap();
        } else {
            writeln!(
                out,
                "Doctor found issues in {issues} {}.",
                if issues == 1 { "category" } else { "categories" }
            )
            .unwrap();

            let errors: Vec<_> = self
                .checks
                .iter()
                .filter(|c| c.status == CheckStatus::Error)
                .collect();
            if !errors.is_empty() {
                writeln!(out).unwrap();
                writeln!(out, "{b}Errors:{r}", b = s.bold, r = s.reset).unwrap();
                for check in &errors {
                    write!(out, "  • {}", check.name).unwrap();
                    if let Some(ref rem) = check.remediation {
                        write!(out, " — {rem}").unwrap();
                    }
                    writeln!(out).unwrap();
                }
            }

            let warnings: Vec<_> = self
                .checks
                .iter()
                .filter(|c| c.status == CheckStatus::Warning)
                .collect();
            if !warnings.is_empty() {
                writeln!(out).unwrap();
                writeln!(out, "{b}Warnings:{r}", b = s.bold, r = s.reset).unwrap();
                for check in &warnings {
                    write!(out, "  • {}", check.name).unwrap();
                    if let Some(ref rem) = check.remediation {
                        write!(out, " — {rem}").unwrap();
                    }
                    writeln!(out).unwrap();
                }
            }
        }

        out
    }
}

// ---------------------------------------------------------------------------
// Check functions (pure, testable)
// ---------------------------------------------------------------------------

pub fn check_config(path: Option<PathBuf>) -> CheckResult {
    match path {
        Some(p) => CheckResult {
            name: "Configuration".to_string(),
            status: CheckStatus::Pass,
            summary: p.display().to_string(),
            details: vec![CheckDetail {
                text: format!("Loaded from {}", p.display()),
            }],
            remediation: None,
        },
        None => CheckResult {
            name: "Configuration".to_string(),
            status: CheckStatus::Warning,
            summary: "no config file found".to_string(),
            details: vec![CheckDetail {
                text: "Create ~/.arc/arc.toml to configure Arc".to_string(),
            }],
            remediation: Some("Create ~/.arc/arc.toml".to_string()),
        },
    }
}

pub fn check_llm_providers(statuses: &[(Provider, bool)]) -> CheckResult {
    let configured: Vec<_> = statuses.iter().filter(|(_, set)| *set).collect();
    let total = statuses.len();
    let count = configured.len();

    let details: Vec<CheckDetail> = statuses
        .iter()
        .map(|(provider, set)| {
            let env_vars = provider.api_key_env_vars().join(" or ");
            let status_text = if *set { "set" } else { "not set" };
            CheckDetail {
                text: format!("{provider} ({env_vars}): {status_text}"),
            }
        })
        .collect();

    if count == 0 {
        CheckResult {
            name: "LLM providers".to_string(),
            status: CheckStatus::Error,
            summary: format!("{count} of {total} configured"),
            details,
            remediation: Some("Set at least one provider API key".to_string()),
        }
    } else {
        CheckResult {
            name: "LLM providers".to_string(),
            status: CheckStatus::Pass,
            summary: format!("{count} of {total} configured"),
            details,
            remediation: None,
        }
    }
}

pub fn check_brave_search(api_key_set: bool) -> CheckResult {
    if api_key_set {
        CheckResult {
            name: "Brave Search".to_string(),
            status: CheckStatus::Pass,
            summary: "API key set".to_string(),
            details: vec![CheckDetail {
                text: "BRAVE_SEARCH_API_KEY is set".to_string(),
            }],
            remediation: None,
        }
    } else {
        CheckResult {
            name: "Brave Search".to_string(),
            status: CheckStatus::Warning,
            summary: "not configured".to_string(),
            details: vec![CheckDetail {
                text: "BRAVE_SEARCH_API_KEY is not set".to_string(),
            }],
            remediation: Some("Set BRAVE_SEARCH_API_KEY to enable web search".to_string()),
        }
    }
}

pub struct SandboxStatus {
    pub daytona: Option<Result<(), String>>,
    pub docker: Option<Result<(), String>>,
}

pub fn check_sandbox(status: &SandboxStatus) -> CheckResult {
    let mut available = Vec::new();
    let mut details = Vec::new();
    let mut errors = Vec::new();

    match &status.daytona {
        Some(Ok(())) => {
            available.push("Daytona");
            details.push(CheckDetail {
                text: "Daytona (DAYTONA_API_KEY): available".to_string(),
            });
        }
        Some(Err(e)) => {
            errors.push(format!("Daytona: {e}"));
            details.push(CheckDetail {
                text: format!("Daytona (DAYTONA_API_KEY): error — {e}"),
            });
        }
        None => {
            details.push(CheckDetail {
                text: "Daytona (DAYTONA_API_KEY): not configured".to_string(),
            });
        }
    }

    match &status.docker {
        Some(Ok(())) => {
            available.push("Docker");
            details.push(CheckDetail {
                text: "Docker: available".to_string(),
            });
        }
        Some(Err(e)) => {
            errors.push(format!("Docker: {e}"));
            details.push(CheckDetail {
                text: format!("Docker: error — {e}"),
            });
        }
        None => {
            details.push(CheckDetail {
                text: "Docker: not available".to_string(),
            });
        }
    }

    if !errors.is_empty() {
        CheckResult {
            name: "Sandbox".to_string(),
            status: CheckStatus::Error,
            summary: errors.join("; "),
            details,
            remediation: Some("Fix sandbox configuration errors".to_string()),
        }
    } else if available.is_empty() {
        CheckResult {
            name: "Sandbox".to_string(),
            status: CheckStatus::Warning,
            summary: "no sandbox available".to_string(),
            details,
            remediation: Some(
                "Install Docker or set DAYTONA_API_KEY to enable sandboxed execution".to_string(),
            ),
        }
    } else {
        CheckResult {
            name: "Sandbox".to_string(),
            status: CheckStatus::Pass,
            summary: format!("{} available", available.join(" + ")),
            details,
            remediation: None,
        }
    }
}

pub struct GithubAppStatus {
    pub app_id: bool,
    pub client_id: bool,
    pub client_secret: bool,
    pub webhook_secret: bool,
    pub private_key: bool,
}

impl GithubAppStatus {
    fn all_set(&self) -> bool {
        self.app_id
            && self.client_id
            && self.client_secret
            && self.webhook_secret
            && self.private_key
    }

    fn none_set(&self) -> bool {
        !self.app_id
            && !self.client_id
            && !self.client_secret
            && !self.webhook_secret
            && !self.private_key
    }
}

pub fn check_github_app(status: &GithubAppStatus) -> CheckResult {
    let fields = [
        ("git.app_id", status.app_id),
        ("git.client_id", status.client_id),
        ("GITHUB_APP_CLIENT_SECRET", status.client_secret),
        ("GITHUB_APP_WEBHOOK_SECRET", status.webhook_secret),
        ("GITHUB_APP_PRIVATE_KEY", status.private_key),
    ];

    let details: Vec<CheckDetail> = fields
        .iter()
        .map(|(name, set)| CheckDetail {
            text: format!("{name}: {}", if *set { "set" } else { "not set" }),
        })
        .collect();

    if status.all_set() {
        CheckResult {
            name: "GitHub App".to_string(),
            status: CheckStatus::Pass,
            summary: "fully configured".to_string(),
            details,
            remediation: None,
        }
    } else if status.none_set() {
        CheckResult {
            name: "GitHub App".to_string(),
            status: CheckStatus::Warning,
            summary: "not configured".to_string(),
            details,
            remediation: Some(
                "Configure GitHub App in arc.toml and set env vars to enable GitHub integration"
                    .to_string(),
            ),
        }
    } else {
        let missing: Vec<_> = fields
            .iter()
            .filter(|(_, set)| !set)
            .map(|(name, _)| *name)
            .collect();
        CheckResult {
            name: "GitHub App".to_string(),
            status: CheckStatus::Error,
            summary: "partially configured".to_string(),
            details,
            remediation: Some(format!("Missing: {}", missing.join(", "))),
        }
    }
}

pub struct ApiStatus {
    pub base_url: String,
    pub authentication_strategy: String,
}

pub fn check_api(status: &ApiStatus) -> CheckResult {
    CheckResult {
        name: "Arc API".to_string(),
        status: CheckStatus::Pass,
        summary: status.base_url.clone(),
        details: vec![
            CheckDetail {
                text: format!("Base URL: {}", status.base_url),
            },
            CheckDetail {
                text: format!("Authentication: {}", status.authentication_strategy),
            },
        ],
        remediation: None,
    }
}

pub struct WebStatus {
    pub url: String,
    pub auth_provider: String,
    pub allowed_usernames_count: usize,
}

pub fn check_web(status: &WebStatus) -> CheckResult {
    CheckResult {
        name: "Arc Web".to_string(),
        status: CheckStatus::Pass,
        summary: status.url.clone(),
        details: vec![
            CheckDetail {
                text: format!("URL: {}", status.url),
            },
            CheckDetail {
                text: format!("Auth provider: {}", status.auth_provider),
            },
            CheckDetail {
                text: format!("Allowed usernames: {}", status.allowed_usernames_count),
            },
        ],
        remediation: None,
    }
}

// ---------------------------------------------------------------------------
// Orchestrator (does real I/O)
// ---------------------------------------------------------------------------

async fn probe_daytona() -> Option<Result<(), String>> {
    if std::env::var("DAYTONA_API_KEY").is_err() {
        return None;
    }
    Some(
        daytona_sdk::Client::new()
            .await
            .map(|_| ())
            .map_err(|e| e.to_string()),
    )
}

async fn probe_docker() -> Option<Result<(), String>> {
    let docker = bollard::Docker::connect_with_local_defaults()
        .map_err(|e| e.to_string())
        .ok()?;
    Some(docker.ping().await.map(|_| ()).map_err(|e| e.to_string()))
}

pub async fn run_doctor(verbose: bool) -> i32 {
    let styles = Styles::detect_stdout();

    // Gather state
    let config_path = dirs::home_dir().map(|h| h.join(".arc").join("arc.toml"));
    let config_exists = config_path
        .as_ref()
        .is_some_and(|p| p.exists());

    let llm_statuses: Vec<(Provider, bool)> = Provider::ALL
        .iter()
        .map(|p| (*p, p.has_api_key()))
        .collect();

    let brave_key_set = std::env::var("BRAVE_SEARCH_API_KEY").is_ok();

    let server_config = arc_api::server_config::load_server_config()
        .unwrap_or_default();

    let api_status = ApiStatus {
        base_url: server_config.api.base_url.clone(),
        authentication_strategy: match server_config.api.authentication_strategy {
            ApiAuthenticationStrategy::Jwt => "jwt".to_string(),
            ApiAuthenticationStrategy::InsecureDisabled => "insecure_disabled".to_string(),
        },
    };

    let web_status = WebStatus {
        url: server_config.web.url.clone(),
        auth_provider: match server_config.web.auth.provider {
            AuthProvider::Github => "github".to_string(),
            AuthProvider::InsecureDisabled => "insecure_disabled".to_string(),
        },
        allowed_usernames_count: server_config.web.auth.allowed_usernames.len(),
    };

    let github_status = GithubAppStatus {
        app_id: server_config.git.app_id.is_some(),
        client_id: server_config.git.client_id.is_some(),
        client_secret: std::env::var("GITHUB_APP_CLIENT_SECRET").is_ok(),
        webhook_secret: std::env::var("GITHUB_APP_WEBHOOK_SECRET").is_ok(),
        private_key: std::env::var("GITHUB_APP_PRIVATE_KEY").is_ok(),
    };

    // Probe sandboxes concurrently
    let (daytona_result, docker_result) = tokio::join!(probe_daytona(), probe_docker());
    let sandbox_status = SandboxStatus {
        daytona: daytona_result,
        docker: docker_result,
    };

    // Run pure checks
    let report = DoctorReport {
        checks: vec![
            check_config(if config_exists { config_path } else { None }),
            check_api(&api_status),
            check_web(&web_status),
            check_llm_providers(&llm_statuses),
            check_brave_search(brave_key_set),
            check_sandbox(&sandbox_status),
            check_github_app(&github_status),
        ],
    };

    print!("{}", report.render(&styles, verbose));

    if report.has_errors() { 1 } else { 0 }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn pass_check(name: &str) -> CheckResult {
        CheckResult {
            name: name.to_string(),
            status: CheckStatus::Pass,
            summary: "all good".to_string(),
            details: vec![CheckDetail {
                text: "everything is fine".to_string(),
            }],
            remediation: None,
        }
    }

    fn warning_check(name: &str) -> CheckResult {
        CheckResult {
            name: name.to_string(),
            status: CheckStatus::Warning,
            summary: "not configured".to_string(),
            details: vec![CheckDetail {
                text: "missing something".to_string(),
            }],
            remediation: Some("fix it".to_string()),
        }
    }

    fn error_check(name: &str) -> CheckResult {
        CheckResult {
            name: name.to_string(),
            status: CheckStatus::Error,
            summary: "broken".to_string(),
            details: vec![CheckDetail {
                text: "something is wrong".to_string(),
            }],
            remediation: Some("repair it".to_string()),
        }
    }

    // -- render: all-pass, no color --

    #[test]
    fn render_all_pass_no_color() {
        let report = DoctorReport {
            checks: vec![pass_check("Test")],
        };
        let out = report.render(&Styles::new(false), false);
        assert!(out.contains("[✓]"));
        assert!(out.contains("All checks passed."));
        assert!(out.contains("Arc Doctor"));
    }

    // -- render: warning footer --

    #[test]
    fn render_warning_footer() {
        let report = DoctorReport {
            checks: vec![warning_check("Optional")],
        };
        let out = report.render(&Styles::new(false), false);
        assert!(out.contains("[!]"));
        assert!(out.contains("Doctor found issues in 1 category."));
        assert!(out.contains("Warnings:"));
        assert!(out.contains("fix it"));
    }

    // -- render: error footer --

    #[test]
    fn render_error_footer() {
        let report = DoctorReport {
            checks: vec![error_check("Broken")],
        };
        let out = report.render(&Styles::new(false), false);
        assert!(out.contains("[✗]"));
        assert!(out.contains("Errors:"));
        assert!(out.contains("repair it"));
    }

    // -- render: verbose mode --

    #[test]
    fn render_verbose_shows_details() {
        let report = DoctorReport {
            checks: vec![pass_check("Verbose")],
        };
        let out = report.render(&Styles::new(false), true);
        assert!(out.contains("•"));
        assert!(out.contains("everything is fine"));
    }

    #[test]
    fn render_default_hides_details() {
        let report = DoctorReport {
            checks: vec![pass_check("Verbose")],
        };
        let out = report.render(&Styles::new(false), false);
        assert!(!out.contains("everything is fine"));
    }

    // -- render: color --

    #[test]
    fn render_color_pass_green() {
        let report = DoctorReport {
            checks: vec![pass_check("Color")],
        };
        let out = report.render(&Styles::new(true), false);
        assert!(out.contains("\x1b[32m")); // green
    }

    #[test]
    fn render_color_warning_yellow() {
        let report = DoctorReport {
            checks: vec![warning_check("Color")],
        };
        let out = report.render(&Styles::new(true), false);
        assert!(out.contains("\x1b[33m")); // yellow
    }

    #[test]
    fn render_color_error_red() {
        let report = DoctorReport {
            checks: vec![error_check("Color")],
        };
        let out = report.render(&Styles::new(true), false);
        assert!(out.contains("\x1b[31m")); // red
    }

    // -- has_errors / issue_count --

    #[test]
    fn has_errors_false_for_warnings_only() {
        let report = DoctorReport {
            checks: vec![pass_check("OK"), warning_check("Warn")],
        };
        assert!(!report.has_errors());
    }

    #[test]
    fn has_errors_true_when_error_present() {
        let report = DoctorReport {
            checks: vec![pass_check("OK"), error_check("Broken")],
        };
        assert!(report.has_errors());
    }

    #[test]
    fn issue_count_counts_warnings_and_errors() {
        let report = DoctorReport {
            checks: vec![
                pass_check("OK"),
                warning_check("Warn"),
                error_check("Broken"),
            ],
        };
        assert_eq!(report.issue_count(), 2);
    }

    // -- check_config --

    #[test]
    fn check_config_pass_with_path() {
        let result = check_config(Some(PathBuf::from("/home/user/.arc/arc.toml")));
        assert_eq!(result.status, CheckStatus::Pass);
        assert!(result.summary.contains(".arc/arc.toml"));
    }

    #[test]
    fn check_config_warning_without_path() {
        let result = check_config(None);
        assert_eq!(result.status, CheckStatus::Warning);
        assert!(result.remediation.is_some());
    }

    // -- check_llm_providers --

    #[test]
    fn check_llm_all_configured() {
        let statuses: Vec<(Provider, bool)> =
            Provider::ALL.iter().map(|p| (*p, true)).collect();
        let result = check_llm_providers(&statuses);
        assert_eq!(result.status, CheckStatus::Pass);
        assert!(result.summary.contains("7 of 7"));
    }

    #[test]
    fn check_llm_some_configured() {
        let mut statuses: Vec<(Provider, bool)> =
            Provider::ALL.iter().map(|p| (*p, false)).collect();
        statuses[0].1 = true; // Anthropic
        statuses[1].1 = true; // OpenAi
        statuses[2].1 = true; // Gemini
        statuses[3].1 = true; // Kimi
        statuses[4].1 = true; // Zai
        let result = check_llm_providers(&statuses);
        assert_eq!(result.status, CheckStatus::Pass);
        assert!(result.summary.contains("5 of 7"));
    }

    #[test]
    fn check_llm_none_configured() {
        let statuses: Vec<(Provider, bool)> =
            Provider::ALL.iter().map(|p| (*p, false)).collect();
        let result = check_llm_providers(&statuses);
        assert_eq!(result.status, CheckStatus::Error);
        assert!(result.summary.contains("0 of 7"));
    }

    // -- check_brave_search --

    #[test]
    fn check_brave_configured() {
        let result = check_brave_search(true);
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn check_brave_not_configured() {
        let result = check_brave_search(false);
        assert_eq!(result.status, CheckStatus::Warning);
        assert!(result.remediation.is_some());
    }

    // -- check_sandbox --

    #[test]
    fn check_sandbox_daytona_ok() {
        let status = SandboxStatus {
            daytona: Some(Ok(())),
            docker: None,
        };
        let result = check_sandbox(&status);
        assert_eq!(result.status, CheckStatus::Pass);
        assert!(result.summary.contains("Daytona available"));
    }

    #[test]
    fn check_sandbox_docker_ok() {
        let status = SandboxStatus {
            daytona: None,
            docker: Some(Ok(())),
        };
        let result = check_sandbox(&status);
        assert_eq!(result.status, CheckStatus::Pass);
        assert!(result.summary.contains("Docker available"));
    }

    #[test]
    fn check_sandbox_both_ok() {
        let status = SandboxStatus {
            daytona: Some(Ok(())),
            docker: Some(Ok(())),
        };
        let result = check_sandbox(&status);
        assert_eq!(result.status, CheckStatus::Pass);
        assert!(result.summary.contains("Daytona + Docker available"));
    }

    #[test]
    fn check_sandbox_both_unavailable() {
        let status = SandboxStatus {
            daytona: None,
            docker: None,
        };
        let result = check_sandbox(&status);
        assert_eq!(result.status, CheckStatus::Warning);
    }

    #[test]
    fn check_sandbox_configured_but_broken() {
        let status = SandboxStatus {
            daytona: Some(Err("connection refused".to_string())),
            docker: None,
        };
        let result = check_sandbox(&status);
        assert_eq!(result.status, CheckStatus::Error);
    }

    // -- check_github_app --

    #[test]
    fn check_github_all_set() {
        let status = GithubAppStatus {
            app_id: true,
            client_id: true,
            client_secret: true,
            webhook_secret: true,
            private_key: true,
        };
        let result = check_github_app(&status);
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn check_github_none_set() {
        let status = GithubAppStatus {
            app_id: false,
            client_id: false,
            client_secret: false,
            webhook_secret: false,
            private_key: false,
        };
        let result = check_github_app(&status);
        assert_eq!(result.status, CheckStatus::Warning);
    }

    #[test]
    fn check_github_partial() {
        let status = GithubAppStatus {
            app_id: true,
            client_id: true,
            client_secret: false,
            webhook_secret: false,
            private_key: false,
        };
        let result = check_github_app(&status);
        assert_eq!(result.status, CheckStatus::Error);
        let rem = result.remediation.unwrap();
        assert!(rem.contains("GITHUB_APP_CLIENT_SECRET"));
        assert!(rem.contains("GITHUB_APP_WEBHOOK_SECRET"));
        assert!(rem.contains("GITHUB_APP_PRIVATE_KEY"));
    }

    // -- check_api --

    #[test]
    fn check_api_shows_base_url() {
        let status = ApiStatus {
            base_url: "http://localhost:3000".to_string(),
            authentication_strategy: "jwt".to_string(),
        };
        let result = check_api(&status);
        assert_eq!(result.status, CheckStatus::Pass);
        assert_eq!(result.summary, "http://localhost:3000");
    }

    #[test]
    fn check_api_details_show_auth_strategy() {
        let status = ApiStatus {
            base_url: "https://api.example.com".to_string(),
            authentication_strategy: "jwt".to_string(),
        };
        let result = check_api(&status);
        assert!(result.details.iter().any(|d| d.text.contains("jwt")));
        assert!(result
            .details
            .iter()
            .any(|d| d.text.contains("https://api.example.com")));
    }

    // -- check_web --

    #[test]
    fn check_web_shows_url() {
        let status = WebStatus {
            url: "http://localhost:5173".to_string(),
            auth_provider: "github".to_string(),
            allowed_usernames_count: 0,
        };
        let result = check_web(&status);
        assert_eq!(result.status, CheckStatus::Pass);
        assert_eq!(result.summary, "http://localhost:5173");
    }

    #[test]
    fn check_web_details_show_auth() {
        let status = WebStatus {
            url: "https://arc.example.com".to_string(),
            auth_provider: "github".to_string(),
            allowed_usernames_count: 3,
        };
        let result = check_web(&status);
        assert!(result.details.iter().any(|d| d.text.contains("github")));
        assert!(result
            .details
            .iter()
            .any(|d| d.text.contains("https://arc.example.com")));
        assert!(result
            .details
            .iter()
            .any(|d| d.text.contains("Allowed usernames: 3")));
    }

    // -- render: multiple issues --

    #[test]
    fn render_multiple_issues_pluralizes() {
        let report = DoctorReport {
            checks: vec![warning_check("A"), error_check("B")],
        };
        let out = report.render(&Styles::new(false), false);
        assert!(out.contains("2 categories"));
    }
}
