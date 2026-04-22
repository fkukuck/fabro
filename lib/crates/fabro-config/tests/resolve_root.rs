use fabro_config::parse_settings_layer;
use fabro_types::settings::{InterpString, SettingsLayer};

fn parse(source: &str) -> SettingsLayer {
    parse_settings_layer(source).expect("fixture should parse")
}

#[test]
fn resolves_root_settings_require_explicit_server_auth_methods() {
    let errors = fabro_config::resolve_server_from_file(&SettingsLayer::default())
        .expect_err("empty server settings should fail");

    assert!(errors.iter().any(|error| {
        matches!(
            error,
            fabro_config::ResolveError::Missing { path } if path == "server.auth.methods"
        )
    }));
}

#[test]
fn resolve_accumulates_errors_across_namespaces() {
    let settings = parse(
        r#"
_version = 1

[server.listen]
type = "tcp"
address = "not-a-socket-addr"

[server.auth]
methods = ["github"]

[server.auth.github]
allowed_usernames = []

[run.sandbox]
provider = "not-a-provider"
"#,
    );

    let mut rendered = Vec::new();
    rendered.extend(
        fabro_config::resolve_server_from_file(&settings)
            .expect_err("invalid server settings should fail")
            .into_iter()
            .map(|error| error.to_string()),
    );
    rendered.extend(
        fabro_config::resolve_run_from_file(&settings)
            .expect_err("invalid run settings should fail")
            .into_iter()
            .map(|error| error.to_string()),
    );
    let rendered = rendered.join("\n");

    assert!(rendered.contains("server.listen.address"));
    assert!(rendered.contains("server.auth.github.allowed_usernames"));
    assert!(rendered.contains("run.sandbox.provider"));
}

#[test]
fn namespace_resolvers_cover_root_level_settings_shape() {
    let settings = parse(
        r#"
_version = 1

[project]
directory = ".fabro"

[workflow]
graph = "graphs/workflow.dot"

[server.storage]
root = "/srv/fabro"

[server.auth]
methods = ["dev-token"]
[run.model]
provider = "openai"
name = "gpt-5"
"#,
    );

    let project = fabro_config::resolve_project_from_file(&settings)
        .expect("project settings should resolve");
    let workflow = fabro_config::resolve_workflow_from_file(&settings)
        .expect("workflow settings should resolve");
    let server =
        fabro_config::resolve_server_from_file(&settings).expect("server settings should resolve");
    let run = fabro_config::resolve_run_from_file(&settings).expect("run settings should resolve");

    assert_eq!(project.directory, ".fabro");
    assert_eq!(workflow.graph, "graphs/workflow.dot");
    assert_eq!(server.storage.root.as_source(), "/srv/fabro");
    assert_eq!(
        run.model.provider.as_ref().map(InterpString::as_source),
        Some("openai".to_string())
    );
    assert_eq!(
        run.model.name.as_ref().map(InterpString::as_source),
        Some("gpt-5".to_string())
    );
}
