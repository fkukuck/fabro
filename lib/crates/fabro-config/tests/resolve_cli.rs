use fabro_config::{parse_settings_layer, resolve_cli_from_file};
use fabro_types::settings::cli::{CliTargetSettings, OutputFormat, OutputVerbosity};
use fabro_types::settings::run::AgentPermissions;
use fabro_types::settings::{InterpString, SettingsLayer};
use temp_env::with_var;

#[test]
fn resolves_cli_defaults_from_empty_settings() {
    let settings = SettingsLayer::default();

    let cli = resolve_cli_from_file(&settings).expect("empty settings should resolve");

    assert!(cli.target.is_none());
    assert_eq!(cli.output.format, OutputFormat::Text);
    assert_eq!(cli.output.verbosity, OutputVerbosity::Normal);
    assert!(!cli.exec.prevent_idle_sleep);
    assert!(cli.updates.check);
    assert!(cli.logging.level.is_none());
}

#[test]
fn user_settings_from_layer_matches_namespace_resolvers() {
    let settings: SettingsLayer = parse_settings_layer(
        r#"
_version = 1

[cli.target]
type = "http"
url = "https://config.example.com"

[features]
session_sandboxes = true
"#,
    )
    .expect("fixture should parse");

    let user_settings =
        fabro_config::UserSettings::from_layer(&settings).expect("user settings should resolve");

    assert_eq!(
        user_settings.cli,
        resolve_cli_from_file(&settings).expect("cli namespace should resolve")
    );
    assert_eq!(
        user_settings.features,
        fabro_config::resolve_features_from_file(&settings)
            .expect("features namespace should resolve")
    );
}

#[test]
fn user_settings_resolve_reads_default_settings_from_fabro_home() {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(
        home.path().join("settings.toml"),
        r#"
_version = 1

[cli.output]
verbosity = "verbose"

[features]
session_sandboxes = true
"#,
    )
    .unwrap();

    with_var("FABRO_HOME", Some(home.path()), || {
        let user_settings =
            fabro_config::UserSettings::resolve().expect("user settings should resolve");
        assert_eq!(user_settings.cli.output.verbosity, OutputVerbosity::Verbose);
        assert!(user_settings.features.session_sandboxes);
    });
}

#[test]
fn user_settings_resolve_returns_defaults_when_default_settings_file_is_missing() {
    let home = tempfile::tempdir().unwrap();

    with_var("FABRO_HOME", Some(home.path()), || {
        let user_settings =
            fabro_config::UserSettings::resolve().expect("user settings should resolve");
        assert_eq!(user_settings.cli.output.format, OutputFormat::Text);
        assert_eq!(user_settings.cli.output.verbosity, OutputVerbosity::Normal);
        assert!(!user_settings.features.session_sandboxes);
    });
}

#[test]
fn resolves_cli_target_exec_and_output_settings() {
    let settings: SettingsLayer = parse_settings_layer(
        r#"
_version = 1

[cli.target]
type = "http"
url = "https://config.example.com"

[cli.exec]
prevent_idle_sleep = true

[cli.exec.model]
provider = "openai"
name = "gpt-5"

[cli.exec.agent]
permissions = "read-only"

[cli.exec.agent.mcps.fs]
type = "stdio"
command = ["echo", "cli"]

[cli.output]
format = "json"
verbosity = "verbose"

[cli.updates]
check = false

[cli.logging]
level = "debug"
"#,
    )
    .expect("fixture should parse");

    let cli = resolve_cli_from_file(&settings).expect("cli settings should resolve");

    let CliTargetSettings::Http { url } = cli.target.expect("target") else {
        panic!("expected http target");
    };
    assert_eq!(url.as_source(), "https://config.example.com");

    assert!(cli.exec.prevent_idle_sleep);
    assert_eq!(
        cli.exec
            .model
            .provider
            .as_ref()
            .map(InterpString::as_source),
        Some("openai".to_string())
    );
    assert_eq!(
        cli.exec.model.name.as_ref().map(InterpString::as_source),
        Some("gpt-5".to_string())
    );
    assert_eq!(cli.exec.agent.permissions, Some(AgentPermissions::ReadOnly));
    assert_eq!(cli.exec.agent.mcps["fs"].name, "fs");
    assert_eq!(cli.output.format, OutputFormat::Json);
    assert_eq!(cli.output.verbosity, OutputVerbosity::Verbose);
    assert!(!cli.updates.check);
    assert_eq!(cli.logging.level.as_deref(), Some("debug"));
}
