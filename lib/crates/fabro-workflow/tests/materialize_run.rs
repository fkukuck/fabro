use std::collections::HashMap;

use fabro_graphviz::graph::Graph;
use fabro_graphviz::parser;
use fabro_model::{Catalog, Provider};
use fabro_types::settings::run::{
    RunGoalLayer, RunLayer, RunModelLayer, RunPullRequestLayer, RunScmLayer, ScmGitHubLayer,
};
use fabro_types::settings::{InterpString, SettingsLayer};
use fabro_workflow::run_materialization::materialize_run;

fn graph(source: &str) -> Graph {
    parser::parse(source).expect("graph should parse")
}

#[test]
fn materialize_run_applies_graph_and_catalog_defaults() {
    let source = r#"digraph Test {
        graph [goal="Build feature"]
        start [shape=Mdiamond]
        exit  [shape=Msquare]
        start -> exit
    }"#;

    let settings = SettingsLayer {
        run: Some(RunLayer {
            model: Some(RunModelLayer {
                name: Some(InterpString::parse("sonnet")),
                ..RunModelLayer::default()
            }),
            pull_request: Some(RunPullRequestLayer {
                enabled: Some(false),
                ..RunPullRequestLayer::default()
            }),
            ..RunLayer::default()
        }),
        ..SettingsLayer::default()
    };

    let materialized = materialize_run(settings, &graph(source), Catalog::builtin(), &[]);
    let resolved = fabro_config::resolve_run_from_file(&materialized).unwrap();

    assert_eq!(
        resolved
            .model
            .name
            .as_ref()
            .map(InterpString::as_source)
            .as_deref(),
        Some("claude-sonnet-4-6")
    );
    assert_eq!(
        resolved
            .model
            .provider
            .as_ref()
            .map(InterpString::as_source)
            .as_deref(),
        Some("anthropic")
    );
    assert_eq!(
        materialized.run.as_ref().and_then(|run| run.goal.as_ref()),
        Some(&RunGoalLayer::Inline(InterpString::parse("Build feature")))
    );
    assert!(resolved.pull_request.is_none());
}

#[test]
fn materialize_run_uses_configured_provider_defaults() {
    let source = r#"digraph Test {
        graph [goal="Build feature"]
        start [shape=Mdiamond]
        exit  [shape=Msquare]
        start -> exit
    }"#;

    let materialized = materialize_run(
        SettingsLayer::default(),
        &graph(source),
        Catalog::builtin(),
        &[Provider::OpenAi],
    );
    let resolved = fabro_config::resolve_run_from_file(&materialized).unwrap();

    assert_eq!(
        resolved
            .model
            .provider
            .as_ref()
            .map(InterpString::as_source)
            .as_deref(),
        Some("openai")
    );
}

#[test]
fn materialize_run_preserves_run_scm_github_permissions() {
    let source = r#"digraph Test {
        graph [goal="Build feature"]
        start [shape=Mdiamond]
        exit  [shape=Msquare]
        start -> exit
    }"#;

    let settings = SettingsLayer {
        run: Some(RunLayer {
            scm: Some(RunScmLayer {
                github: Some(ScmGitHubLayer {
                    permissions: HashMap::from([
                        ("contents".to_string(), InterpString::parse("write")),
                        ("issues".to_string(), InterpString::parse("read")),
                    ]),
                }),
                ..RunScmLayer::default()
            }),
            ..RunLayer::default()
        }),
        ..SettingsLayer::default()
    };

    let materialized = materialize_run(settings, &graph(source), Catalog::builtin(), &[]);

    let permissions = &materialized
        .run
        .as_ref()
        .and_then(|run| run.scm.as_ref())
        .and_then(|scm| scm.github.as_ref())
        .expect("github scm layer should be preserved")
        .permissions;

    assert_eq!(
        permissions
            .get("contents")
            .map(InterpString::as_source)
            .as_deref(),
        Some("write")
    );
    assert_eq!(
        permissions
            .get("issues")
            .map(InterpString::as_source)
            .as_deref(),
        Some("read")
    );
}
