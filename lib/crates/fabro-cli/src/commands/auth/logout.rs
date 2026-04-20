use anyhow::{Result, bail};
use fabro_http::header::AUTHORIZATION;
use fabro_types::settings::CliSettings;
use fabro_types::settings::cli::CliLayer;
use fabro_util::printer::Printer;

use crate::args::{AuthLogoutArgs, require_no_json_override};
use crate::auth_store::{AuthEntry, AuthStore, ServerTargetKey};
use crate::command_context::CommandContext;
use crate::user_config;
use crate::user_config::ServerTarget;

pub(super) async fn logout_command(
    args: AuthLogoutArgs,
    cli: &CliSettings,
    cli_layer: &CliLayer,
    process_local_json: bool,
    printer: Printer,
) -> Result<()> {
    require_no_json_override(process_local_json)?;

    let ctx = CommandContext::base(printer, cli.clone(), cli_layer)?;
    let store = AuthStore::default();
    if args.all {
        let entries = store.list()?;
        if entries.is_empty() {
            fabro_util::printerr!(printer, "Not logged in to any servers.");
            return Ok(());
        }

        let mut warnings = Vec::new();
        for (key, entry) in entries {
            if let Ok(target) = server_target_from_key(&key) {
                if let Err(error) = revoke_remote_session(&target, &entry).await {
                    warnings.push(format_warning(&key, &error.to_string()));
                }
            }
            store.remove(&key)?;
        }

        for warning in warnings {
            fabro_util::printerr!(printer, "{warning}");
        }
        fabro_util::printerr!(printer, "Cleared local CLI auth sessions.");
        return Ok(());
    }

    let target = user_config::resolve_server_target(&args.server, ctx.machine_settings())?;
    let key = ServerTargetKey::new(&target)?;
    let Some(entry) = store.get(&key)? else {
        fabro_util::printerr!(printer, "Not logged in to {}.", key);
        return Ok(());
    };

    if let Err(error) = revoke_remote_session(&target, &entry).await {
        fabro_util::printerr!(printer, "{}", format_warning(&key, &error.to_string()));
    }
    store.remove(&key)?;
    fabro_util::printerr!(printer, "Logged out from {}.", key);
    Ok(())
}

async fn revoke_remote_session(target: &ServerTarget, entry: &AuthEntry) -> Result<()> {
    let (http_client, base_url) = user_config::build_public_http_client(target)?;
    let response = http_client
        .post(format!("{base_url}/auth/cli/logout"))
        .header(AUTHORIZATION, format!("Bearer {}", entry.refresh_token))
        .send()
        .await?;
    if response.status().is_success() {
        return Ok(());
    }

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if body.is_empty() {
        bail!("request failed with status {status}");
    }
    bail!("request failed with status {status}: {body}")
}

fn server_target_from_key(key: &ServerTargetKey) -> Result<ServerTarget> {
    let value = key.to_string();
    if let Some(path) = value.strip_prefix("unix://") {
        return Ok(ServerTarget::UnixSocket(path.into()));
    }
    if value.starts_with("http://") || value.starts_with("https://") {
        return Ok(ServerTarget::HttpUrl {
            api_url: value,
            tls:     None,
        });
    }
    bail!("invalid auth store server key `{value}`")
}

fn format_warning(key: &ServerTargetKey, error: &str) -> String {
    format!(
        "Warning: removed local session for {key}, but remote revocation failed: {error}. The refresh token may remain valid until it expires."
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{format_warning, server_target_from_key};
    use crate::auth_store::ServerTargetKey;
    use crate::user_config::ServerTarget;

    #[test]
    fn rebuilds_server_target_from_http_key() {
        let key = ServerTargetKey::new(&ServerTarget::HttpUrl {
            api_url: "https://fabro.example.com/api/v1".to_string(),
            tls:     None,
        })
        .unwrap();

        assert_eq!(
            server_target_from_key(&key).unwrap(),
            ServerTarget::HttpUrl {
                api_url: "https://fabro.example.com".to_string(),
                tls:     None,
            }
        );
    }

    #[test]
    fn rebuilds_server_target_from_unix_key() {
        let key = ServerTargetKey::new(&ServerTarget::UnixSocket(PathBuf::from("/tmp/fabro.sock")))
            .unwrap();

        assert_eq!(
            server_target_from_key(&key).unwrap(),
            ServerTarget::UnixSocket(PathBuf::from("/tmp/fabro.sock"))
        );
    }

    #[test]
    fn warning_mentions_local_removal_and_remote_failure() {
        let key = ServerTargetKey::new(&ServerTarget::HttpUrl {
            api_url: "https://fabro.example.com".to_string(),
            tls:     None,
        })
        .unwrap();
        let warning = format_warning(&key, "request failed with status 500");
        assert!(warning.contains("removed local session"));
        assert!(warning.contains("remote revocation failed"));
    }
}
