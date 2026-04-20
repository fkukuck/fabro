use std::net::IpAddr;

use thiserror::Error;

use crate::user_config::{self, ServerTarget};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoopbackClassification {
    Https,
    LoopbackHttp,
    UnixSocket,
    Rejected,
}

#[derive(Debug, Error)]
pub(crate) enum TargetSchemeError {
    #[error("invalid server URL `{value}`: {reason}")]
    InvalidUrl { value: String, reason: String },
    #[error("unsupported server URL scheme `{scheme}`")]
    UnsupportedScheme { scheme: String },
    #[error("server URL `{value}` is missing a host")]
    MissingHost { value: String },
}

pub(crate) fn is_loopback_or_unix_socket(
    target: &ServerTarget,
) -> Result<LoopbackClassification, TargetSchemeError> {
    match target {
        ServerTarget::UnixSocket(_) => Ok(LoopbackClassification::UnixSocket),
        ServerTarget::HttpUrl { api_url, .. } => classify_http_target(api_url),
    }
}

fn classify_http_target(api_url: &str) -> Result<LoopbackClassification, TargetSchemeError> {
    let url = fabro_http::Url::parse(user_config::normalized_http_base_url(api_url)).map_err(
        |source| TargetSchemeError::InvalidUrl {
            value:  api_url.to_string(),
            reason: source.to_string(),
        },
    )?;

    match url.scheme() {
        "https" => Ok(LoopbackClassification::Https),
        "http" => {
            let Some(host) = url.host_str() else {
                return Err(TargetSchemeError::MissingHost {
                    value: api_url.to_string(),
                });
            };
            let classification = host
                .parse::<IpAddr>()
                .ok()
                .filter(ip_is_loopback)
                .map_or(LoopbackClassification::Rejected, |_| {
                    LoopbackClassification::LoopbackHttp
                });
            Ok(classification)
        }
        scheme => Err(TargetSchemeError::UnsupportedScheme {
            scheme: scheme.to_string(),
        }),
    }
}

fn ip_is_loopback(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => ipv4.is_loopback(),
        IpAddr::V6(ipv6) => {
            ipv6.is_loopback() || ipv6.to_ipv4_mapped().is_some_and(|ip| ip.is_loopback())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{LoopbackClassification, is_loopback_or_unix_socket};
    use crate::user_config::ServerTarget;

    #[test]
    fn classifies_https_loopback_and_unix_targets() {
        let cases = [
            (
                ServerTarget::HttpUrl {
                    api_url: "https://fabro.example.com".to_string(),
                    tls:     None,
                },
                LoopbackClassification::Https,
            ),
            (
                ServerTarget::HttpUrl {
                    api_url: "http://127.0.0.1:3000".to_string(),
                    tls:     None,
                },
                LoopbackClassification::LoopbackHttp,
            ),
            (
                ServerTarget::HttpUrl {
                    api_url: "http://[::1]:3000".to_string(),
                    tls:     None,
                },
                LoopbackClassification::LoopbackHttp,
            ),
            (
                ServerTarget::HttpUrl {
                    api_url: "http://[::ffff:127.0.0.1]:3000".to_string(),
                    tls:     None,
                },
                LoopbackClassification::LoopbackHttp,
            ),
            (
                ServerTarget::UnixSocket(PathBuf::from("/tmp/fabro.sock")),
                LoopbackClassification::UnixSocket,
            ),
        ];

        for (target, expected) in cases {
            assert_eq!(is_loopback_or_unix_socket(&target).unwrap(), expected);
        }
    }

    #[test]
    fn rejects_plain_http_non_loopback_targets() {
        let cases = [
            "http://fabro.example.com",
            "http://127.0.0.1.evil.com",
            "http://localhost",
            "http://localhost.evil.com",
            "http://2130706433",
            "http://0x7f000001",
        ];

        for api_url in cases {
            let target = ServerTarget::HttpUrl {
                api_url: api_url.to_string(),
                tls:     None,
            };
            assert_eq!(
                is_loopback_or_unix_socket(&target).unwrap(),
                LoopbackClassification::Rejected
            );
        }
    }

    #[test]
    fn rejects_unsupported_schemes() {
        let target = ServerTarget::HttpUrl {
            api_url: "ftp://fabro.example.com".to_string(),
            tls:     None,
        };
        let error = is_loopback_or_unix_socket(&target).unwrap_err();
        assert!(error.to_string().contains("unsupported server URL scheme"));
    }
}
