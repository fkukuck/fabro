use fabro_test::{fabro_snapshot, run_and_format, test_context};
use insta::assert_snapshot;

use super::support::{
    git_filters, git_stdout, metadata_run_ids, output_stderr as support_stderr,
    run_branch_commits_since_base, run_events, run_state, setup_git_backed_changed_run,
};

#[test]
fn help() {
    let context = test_context!();
    let mut cmd = context.command();
    cmd.args(["rewind", "--help"]);
    fabro_snapshot!(context.filters(), cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----
    Rewind a workflow run to an earlier checkpoint

    Usage: fabro rewind [OPTIONS] <RUN_ID> [TARGET]

    Arguments:
      <RUN_ID>  Run ID (or unambiguous prefix)
      [TARGET]  Target checkpoint: node name, node@visit, or @ordinal (omit with --list)

    Options:
          --json              Output as JSON [env: FABRO_JSON=]
          --server <SERVER>   Fabro server target: http(s) URL or absolute Unix socket path [env: FABRO_SERVER=]
          --debug             Enable DEBUG-level logging (default is INFO) [env: FABRO_DEBUG=]
          --list              Show the checkpoint timeline instead of rewinding
          --no-push           Skip force-pushing rewound refs to the remote
          --no-upgrade-check  Disable automatic upgrade check [env: FABRO_NO_UPGRADE_CHECK=true]
          --quiet             Suppress non-essential output [env: FABRO_QUIET=]
          --verbose           Enable verbose output [env: FABRO_VERBOSE=]
      -h, --help              Print help
    ----- stderr -----
    ");
}

#[test]
fn rewind_outside_git_repo_errors() {
    let context = test_context!();
    let mut cmd = context.command();
    cmd.args(["rewind", "01ARZ3NDEKTSV4RRFFQ69G5FAW", "--list"]);

    fabro_snapshot!(context.filters(), cmd, @"
    success: false
    exit_code: 1
    ----- stdout -----
    ----- stderr -----
      × No run found matching '[ULID]' (tried run ID prefix and workflow name)
    ");
}

#[test]
fn rewind_list_prints_timeline_for_completed_git_run() {
    let context = test_context!();
    let setup = setup_git_backed_changed_run(&context);
    let mut cmd = context.command();
    cmd.current_dir(&setup.repo_dir);
    cmd.args(["rewind", &setup.run.run_id, "--list"]);

    fabro_snapshot!(git_filters(&context), cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----
    ----- stderr -----
    @   Node      Details 
     @1  step_one          
     @2  step_two
    ");
}

#[test]
fn rewind_target_updates_metadata_and_resume_hint() {
    let context = test_context!();
    let setup = setup_git_backed_changed_run(&context);
    let before = metadata_run_ids(&setup.repo_dir);
    let expected_run_head =
        run_branch_commits_since_base(&setup.repo_dir, &setup.run.run_id, &setup.base_sha)
            .into_iter()
            .next()
            .expect("source run should have a first run commit");

    let mut cmd = context.command();
    cmd.current_dir(&setup.repo_dir);
    cmd.args(["rewind", &setup.run.run_id, "@1", "--no-push"]);

    let (snapshot, output) = run_and_format(&mut cmd, &git_filters(&context));
    assert_snapshot!(snapshot, @"
    success: true
    exit_code: 0
    ----- stdout -----
    ----- stderr -----

    Rewound [RUN_PREFIX]; new run [RUN_PREFIX]
    To resume: fabro resume [RUN_PREFIX]
    ");
    assert!(output.status.success(), "rewind should succeed");

    let after = metadata_run_ids(&setup.repo_dir);
    let new_run_ids: Vec<_> = after.difference(&before).cloned().collect();
    assert_eq!(
        new_run_ids.len(),
        1,
        "rewind should create one replacement run"
    );
    let new_run_id = &new_run_ids[0];

    let run_head = git_stdout(&setup.repo_dir, &[
        "rev-parse",
        &format!("fabro/run/{new_run_id}"),
    ]);
    assert_eq!(run_head.trim(), expected_run_head);

    let state = run_state(&setup.run.run_dir);
    assert!(matches!(
        state.status,
        Some(fabro_types::RunStatus::Archived { .. })
    ));
    assert_eq!(
        state.superseded_by.map(|run_id| run_id.to_string()),
        Some(new_run_id.clone())
    );
}

#[test]
fn rewind_archives_source_and_records_superseded_by() {
    let context = test_context!();
    let setup = setup_git_backed_changed_run(&context);
    let before_events = run_events(&setup.run.run_dir);
    assert!(
        before_events
            .iter()
            .any(|event| event.event.event_name() == "run.completed"),
        "setup run should be completed before rewind"
    );

    let mut cmd = context.command();
    cmd.current_dir(&setup.repo_dir);
    cmd.args(["rewind", &setup.run.run_id, "@1", "--no-push"]);
    let output = cmd.output().expect("rewind should execute");
    assert!(
        output.status.success(),
        "rewind should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        support_stderr(&output),
    );

    let after_events = run_events(&setup.run.run_dir);
    assert_eq!(
        after_events.len(),
        before_events.len() + 2,
        "rewind should append run.archived and run.superseded_by"
    );
    assert_eq!(
        after_events[..before_events.len()]
            .iter()
            .map(|event| event.event.event_name())
            .collect::<Vec<_>>(),
        before_events
            .iter()
            .map(|event| event.event.event_name())
            .collect::<Vec<_>>(),
        "rewind should preserve the prior event prefix"
    );
    assert_eq!(
        after_events[before_events.len()].event.event_name(),
        "run.archived"
    );
    assert_eq!(
        after_events[before_events.len() + 1].event.event_name(),
        "run.superseded_by"
    );

    let state = run_state(&setup.run.run_dir);
    assert!(matches!(
        state.status,
        Some(fabro_types::RunStatus::Archived { .. })
    ));
    assert!(state.superseded_by.is_some());
}
