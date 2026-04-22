use fabro_types::settings::ServerNamespace as ResolvedServerSettings;
use url::Url;

use crate::server::EnvLookup;

pub(crate) fn resolve_canonical_origin(
    resolved: &ResolvedServerSettings,
    env_lookup: &EnvLookup,
) -> Result<String, String> {
    let value = resolved
        .web
        .url
        .resolve(|name| env_lookup(name))
        .map_err(|_| canonical_origin_error(&resolved.web.url.as_source()))?
        .value;

    let parsed = Url::parse(&value).map_err(|_| canonical_origin_error(&value))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(canonical_origin_error(&value));
    }

    Ok(value)
}

fn canonical_origin_error(value: &str) -> String {
    format!(
        "server.web.url is required and must be an absolute http(s) URL (got \"{value}\"). Set it in your settings file or via the FABRO_WEB_URL environment variable."
    )
}
