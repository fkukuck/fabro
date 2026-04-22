use serde::Serialize;

use super::{
    CliSettings, FeaturesSettings, ProjectSettings, RunSettings, ServerSettings, WorkflowSettings,
};

/// A fully resolved settings view across all namespaces.
///
/// `Default` is intentionally not derived: a default `Settings` value would
/// contain empty `server.auth.methods`, which the resolver rejects. Construct
/// real values via `fabro_config::resolve` (production), or
/// `Settings::test_default()` behind the `test-support` feature (tests).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Settings {
    pub project:  ProjectSettings,
    pub workflow: WorkflowSettings,
    pub run:      RunSettings,
    pub cli:      CliSettings,
    pub server:   ServerSettings,
    pub features: FeaturesSettings,
}

#[cfg(any(test, feature = "test-support"))]
impl Settings {
    /// A trivial `Settings` value suitable for serialization or destructuring
    /// tests. Server auth methods are empty (would not pass `resolve`);
    /// use this only when the resolver is not in play.
    #[must_use]
    pub fn test_default() -> Self {
        Self {
            project:  ProjectSettings::default(),
            workflow: WorkflowSettings::default(),
            run:      RunSettings::default(),
            cli:      CliSettings::default(),
            server:   ServerSettings::test_default(),
            features: FeaturesSettings::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::Duration as StdDuration;

    use serde_json::json;

    use super::Settings;
    use crate::settings::cli::CliTargetSettings;
    use crate::settings::interp::InterpString;
    use crate::settings::run::{
        DockerfileSource, McpServerSettings, McpTransport, RunAgentSettings, RunGoal, RunSettings,
    };
    use crate::settings::server::{
        ObjectStoreSettings, ServerListenSettings, ServerSettings, ServerSlateDbSettings,
    };

    #[test]
    fn settings_serializes_successfully() {
        serde_json::to_value(Settings::test_default()).expect("resolved settings should serialize");
    }

    #[test]
    fn resolved_enums_use_human_readable_tagged_shapes() {
        assert_eq!(
            serde_json::to_value(CliTargetSettings::Http {
                url: InterpString::parse("https://api.example.com"),
            })
            .unwrap(),
            json!({
                "type": "http",
                "url": "https://api.example.com",
            })
        );

        assert_eq!(
            serde_json::to_value(RunGoal::Inline(InterpString::parse("ship it"))).unwrap(),
            json!({
                "type": "inline",
                "value": "ship it"
            })
        );

        assert_eq!(
            serde_json::to_value(McpTransport::Sandbox {
                command: vec!["fabro-mcp".to_string(), "--serve".to_string()],
                port:    3333,
                env:     HashMap::from([("TOKEN".to_string(), "{{ env.MCP_TOKEN }}".to_string())]),
            })
            .unwrap(),
            json!({
                "type": "sandbox",
                "command": ["fabro-mcp", "--serve"],
                "port": 3333,
                "env": {
                    "TOKEN": "{{ env.MCP_TOKEN }}"
                }
            })
        );

        assert_eq!(
            serde_json::to_value(DockerfileSource::Path {
                path: "Dockerfile".to_string(),
            })
            .unwrap(),
            json!({
                "type": "path",
                "path": "Dockerfile"
            })
        );

        assert_eq!(
            serde_json::to_value(ObjectStoreSettings::S3 {
                bucket:     InterpString::parse("fabro-artifacts"),
                region:     InterpString::parse("us-east-1"),
                endpoint:   Some(InterpString::parse("https://s3.example.com")),
                path_style: true,
            })
            .unwrap(),
            json!({
                "type": "s3",
                "bucket": "fabro-artifacts",
                "region": "us-east-1",
                "endpoint": "https://s3.example.com",
                "path_style": true
            })
        );
    }

    #[test]
    fn socket_addrs_and_std_durations_use_settings_strings() {
        assert_eq!(
            serde_json::to_value(ServerListenSettings::Tcp {
                address: "127.0.0.1:8080".parse().unwrap(),
            })
            .unwrap(),
            json!({
                "type": "tcp",
                "address": "127.0.0.1:8080"
            })
        );

        let settings = Settings {
            server: ServerSettings {
                slatedb: ServerSlateDbSettings {
                    prefix:         InterpString::parse("slatedb/"),
                    store:          ObjectStoreSettings::Local {
                        root: InterpString::parse("/srv/slatedb"),
                    },
                    flush_interval: StdDuration::from_secs(30),
                    disk_cache:     false,
                },
                ..ServerSettings::test_default()
            },
            run: RunSettings {
                agent: RunAgentSettings {
                    mcps: HashMap::from([("sandboxed".to_string(), McpServerSettings {
                        name:                 "sandboxed".to_string(),
                        transport:            McpTransport::Http {
                            url:     "https://mcp.example.com".to_string(),
                            headers: HashMap::from([(
                                "Authorization".to_string(),
                                "Bearer {{ env.MCP_TOKEN }}".to_string(),
                            )]),
                        },
                        startup_timeout_secs: 15,
                        tool_timeout_secs:    90,
                    })]),
                    ..RunAgentSettings::default()
                },
                ..RunSettings::default()
            },
            ..Settings::test_default()
        };

        let value = serde_json::to_value(settings).unwrap();
        assert_eq!(value["server"]["slatedb"]["flush_interval"], "30s");
    }
}
