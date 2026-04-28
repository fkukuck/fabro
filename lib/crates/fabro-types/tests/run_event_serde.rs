use std::collections::BTreeMap;

use fabro_types::graph::Graph;
use fabro_types::run::{DirtyStatus, ForkSourceRef, PreRunGitContext, PreRunPushOutcome};
use fabro_types::run_event::run::RunCreatedProps;
use fabro_types::settings::InterpString;
use fabro_types::settings::run::RunGoal;
use fabro_types::{WorkflowSettings, fixtures};

fn templated_settings() -> WorkflowSettings {
    let mut settings = WorkflowSettings::default();
    settings.run.goal = Some(RunGoal::Inline(InterpString::parse("Ship {{ env.TASK }}")));
    settings
}

#[test]
fn run_created_props_round_trip_templated_settings() {
    let props = RunCreatedProps {
        settings:             templated_settings(),
        graph:                Graph::new("ship"),
        workflow_source:      Some("digraph Ship { start -> exit }".to_string()),
        workflow_config:      Some("[run]\ngoal = \"Ship {{ env.TASK }}\"".to_string()),
        labels:               BTreeMap::from([("team".to_string(), "platform".to_string())]),
        run_dir:              "/tmp/run".to_string(),
        source_directory:     Some("/Users/client/project".to_string()),
        repo_origin_url:      Some("https://github.com/fabro-sh/fabro.git".to_string()),
        base_branch:          Some("main".to_string()),
        workflow_slug:        Some("demo".to_string()),
        db_prefix:            Some("run_".to_string()),
        provenance:           None,
        manifest_blob:        None,
        pre_run_git:          Some(PreRunGitContext {
            display_base_sha: Some("abc123".to_string()),
            local_dirty:      DirtyStatus::Unknown,
            push_outcome:     PreRunPushOutcome::SkippedNoRemote,
        }),
        fork_source_ref:      Some(ForkSourceRef {
            source_run_id:  fixtures::RUN_2,
            checkpoint_sha: "def456".to_string(),
        }),
        checkpoints_disabled: true,
    };

    let json = serde_json::to_value(&props).expect("props should serialize");
    assert!(json.get("working_directory").is_none());
    assert!(json.get("host_repo_path").is_none());
    assert_eq!(json["source_directory"], "/Users/client/project");
    assert_eq!(
        json["pre_run_git"]["push_outcome"]["type"],
        "skipped_no_remote"
    );
    assert_eq!(json["checkpoints_disabled"], true);

    let round_trip: RunCreatedProps =
        serde_json::from_value(json.clone()).expect("props should deserialize");

    assert_eq!(
        serde_json::to_value(&round_trip).expect("round-trip should serialize"),
        json
    );
    assert_eq!(
        round_trip.settings.run.goal,
        Some(RunGoal::Inline(InterpString::parse("Ship {{ env.TASK }}")))
    );
}
