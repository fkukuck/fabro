#![expect(
    clippy::disallowed_methods,
    reason = "integration tests stage fixtures with sync std::fs; test infrastructure, not Tokio-hot path"
)]

use fabro_test::{fabro_snapshot, test_context};

use super::support::{git_stdout, output_stderr, run_state_by_id, setup_git_backed_changed_run};

const SHARED_DAEMON_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

#[test]
fn help() {
    let context = test_context!();
    let mut cmd = context.command();
    cmd.args(["resume", "--help"]);
    fabro_snapshot!(context.filters(), cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----
    Resume an interrupted workflow run

    Usage: fabro resume [OPTIONS] <RUN>

    Arguments:
      <RUN>  Run ID or unambiguous prefix

    Options:
          --json              Output as JSON [env: FABRO_JSON=]
          --server <SERVER>   Fabro server target: http(s) URL or absolute Unix socket path [env: FABRO_SERVER=]
      -d, --detach            Run in the background and print the run ID
          --debug             Enable DEBUG-level logging (default is INFO) [env: FABRO_DEBUG=]
          --no-upgrade-check  Disable automatic upgrade check [env: FABRO_NO_UPGRADE_CHECK=true]
          --quiet             Suppress non-essential output [env: FABRO_QUIET=]
          --verbose           Enable verbose output [env: FABRO_VERBOSE=]
      -h, --help              Print help
    ----- stderr -----
    ");
}

#[test]
fn resume_requires_run_arg() {
    let context = test_context!();
    let mut cmd = context.command();
    cmd.args(["resume"]);
    fabro_snapshot!(context.filters(), cmd, @"
    success: false
    exit_code: 2
    ----- stdout -----
    ----- stderr -----
    error: the following required arguments were not provided:
      <RUN>

    Usage: fabro resume --no-upgrade-check <RUN>

    For more information, try '--help'.
    ");
}

#[test]
fn resume_rewound_run_succeeds() {
    let context = test_context!();
    let setup = setup_git_backed_changed_run(&context);

    let new_run_id = rewind_replacement_run_id(&context, &setup);
    let rewound_head = run_state_by_id(&context, &new_run_id)
        .checkpoint
        .and_then(|checkpoint| checkpoint.git_commit_sha)
        .expect("rewound run should record checkpoint sha");

    let mut resume_cmd = context.command();
    resume_cmd.current_dir(&setup.repo_dir);
    resume_cmd.env("OPENAI_API_KEY", "test");
    resume_cmd.args(["resume", &new_run_id]);
    let resume_output = resume_cmd.output().expect("resume should execute");
    assert!(
        resume_output.status.success(),
        "resume should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&resume_output.stdout),
        output_stderr(&resume_output)
    );

    assert_eq!(
        git_stdout(&setup.repo_dir, &[
            "show",
            &format!("fabro/run/{new_run_id}:story.txt")
        ]),
        "line 1\nline 2\nline 3\n"
    );
    assert_eq!(
        std::fs::read_to_string(setup.repo_dir.join("story.txt")).unwrap(),
        "line 1\n"
    );
    let resumed_head = git_stdout(&setup.repo_dir, &[
        "rev-parse",
        &format!("fabro/run/{new_run_id}"),
    ]);
    assert_ne!(resumed_head.trim(), rewound_head);
}

#[test]
fn resume_detached_does_not_create_launcher_record() {
    let context = test_context!();
    let setup = setup_git_backed_changed_run(&context);

    let new_run_id = rewind_replacement_run_id(&context, &setup);

    let mut resume_cmd = context.command();
    resume_cmd.current_dir(&setup.repo_dir);
    resume_cmd.env("OPENAI_API_KEY", "test");
    resume_cmd.args(["resume", "--detach", &new_run_id]);
    let resume_output = resume_cmd.output().expect("resume should execute");
    assert!(
        resume_output.status.success(),
        "resume should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&resume_output.stdout),
        output_stderr(&resume_output)
    );

    assert!(
        !context
            .storage_dir
            .join("launchers")
            .join(format!("{new_run_id}.json"))
            .exists(),
        "server-owned resume should not create a launcher record"
    );

    context
        .command()
        .args(["wait", &new_run_id])
        .timeout(SHARED_DAEMON_TIMEOUT)
        .assert()
        .success();
}

fn rewind_replacement_run_id(
    context: &fabro_test::TestContext,
    setup: &super::support::GitRunSetup,
) -> String {
    let rewind = context
        .command()
        .current_dir(&setup.repo_dir)
        .args(["rewind", &setup.run.run_id, "@2", "--no-push", "--json"])
        .output()
        .expect("rewind should execute");
    assert!(
        rewind.status.success(),
        "rewind should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&rewind.stdout),
        output_stderr(&rewind)
    );

    let response: serde_json::Value =
        serde_json::from_slice(&rewind.stdout).expect("rewind json should parse");
    assert_eq!(
        response["source_run_id"].as_str(),
        Some(setup.run.run_id.as_str())
    );
    assert_eq!(response["archived"].as_bool(), Some(true));
    response["new_run_id"]
        .as_str()
        .expect("rewind response should include new_run_id")
        .to_string()
}
