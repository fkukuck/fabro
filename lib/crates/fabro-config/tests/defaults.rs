use fabro_config::{
    apply_builtin_defaults, defaults_layer, parse_settings_layer, resolve_run_from_file,
    resolve_server_from_file, resolve_workflow_from_file,
};
use fabro_types::settings::SettingsLayer;
use fabro_types::settings::cli::OutputFormat;
use fabro_types::settings::run::{ApprovalMode, RunMode, WorktreeMode};
use fabro_types::settings::server::ObjectStoreProvider;

fn parse(source: &str) -> SettingsLayer {
    parse_settings_layer(source).expect("fixture should parse")
}

#[test]
fn embedded_defaults_parse_successfully() {
    let defaults = defaults_layer();

    assert_eq!(
        defaults
            .project
            .as_ref()
            .and_then(|project| project.directory.as_deref()),
        Some(".")
    );
    assert_eq!(
        defaults
            .workflow
            .as_ref()
            .and_then(|workflow| workflow.graph.as_deref()),
        Some("workflow.fabro")
    );
}

#[test]
fn apply_builtin_defaults_materializes_expected_layer() {
    let layer = apply_builtin_defaults(SettingsLayer::default());

    assert_eq!(
        layer
            .project
            .as_ref()
            .and_then(|project| project.directory.as_deref()),
        Some(".")
    );
    assert_eq!(
        layer
            .workflow
            .as_ref()
            .and_then(|workflow| workflow.graph.as_deref()),
        Some("workflow.fabro")
    );
    assert_eq!(
        layer
            .run
            .as_ref()
            .and_then(|run| run.execution.as_ref())
            .and_then(|execution| execution.mode),
        Some(RunMode::Normal)
    );
    assert_eq!(
        layer
            .run
            .as_ref()
            .and_then(|run| run.execution.as_ref())
            .and_then(|execution| execution.approval),
        Some(ApprovalMode::Prompt)
    );
    assert_eq!(
        layer
            .run
            .as_ref()
            .and_then(|run| run.sandbox.as_ref())
            .and_then(|sandbox| sandbox.local.as_ref())
            .and_then(|local| local.worktree_mode),
        Some(WorktreeMode::Clean)
    );
    assert_eq!(
        layer
            .cli
            .as_ref()
            .and_then(|cli| cli.output.as_ref())
            .and_then(|output| output.format),
        Some(OutputFormat::Text)
    );
    assert_eq!(
        layer
            .server
            .as_ref()
            .and_then(|server| server.artifacts.as_ref())
            .and_then(|artifacts| artifacts.provider),
        Some(ObjectStoreProvider::Local)
    );
}

#[test]
fn resolve_empty_settings_requires_explicit_server_auth_methods() {
    let errors = resolve_server_from_file(&SettingsLayer::default())
        .expect_err("empty server settings should fail");

    assert!(errors.iter().any(|error| {
        matches!(
            error,
            fabro_config::ResolveError::Missing { path } if path == "server.auth.methods"
        )
    }));
}

#[test]
fn higher_precedence_values_override_builtin_defaults() {
    let layer = parse(
        r#"
_version = 1

[server.auth]
methods = ["dev-token"]

[run.execution]
mode = "dry_run"
"#,
    );

    let workflow = resolve_workflow_from_file(&layer).expect("workflow settings should resolve");
    let run = resolve_run_from_file(&layer).expect("run settings should resolve");

    assert_eq!(run.execution.mode, RunMode::DryRun);
    assert_eq!(run.execution.approval, ApprovalMode::Prompt);
    assert_eq!(workflow.graph, "workflow.fabro");
}
