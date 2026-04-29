#![expect(
    clippy::disallowed_methods,
    reason = "This recovery scenario test uses the real git CLI to set up repository history for end-to-end assertions."
)]

use std::collections::BTreeSet;
use std::path::Path;

use fabro_test::{fabro_snapshot, test_context};
use git2::Repository;

use crate::support::unique_run_id;

fn list_metadata_run_ids(repo_dir: &Path) -> BTreeSet<String> {
    let repo = Repository::discover(repo_dir).expect("recovery fixture should be a git repo");
    repo.references()
        .expect("recovery fixture should list git references")
        .flatten()
        .filter_map(|reference| reference.name().map(ToOwned::to_owned))
        .filter_map(|name| {
            name.strip_prefix("refs/heads/fabro/meta/")
                .map(ToOwned::to_owned)
        })
        .collect()
}

#[expect(
    clippy::disallowed_methods,
    reason = "This sync git integration helper retries metadata branch deletion until libgit2 releases its lock."
)]
fn delete_metadata_branch_when_ready(repo_dir: &Path, run_id: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let repo = Repository::discover(repo_dir).expect("recovery fixture should stay a git repo");
        let mut reference = repo
            .find_reference(&format!("refs/heads/fabro/meta/{run_id}"))
            .expect("metadata branch should exist");
        match reference.delete() {
            Ok(()) => return,
            Err(err) => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "metadata branch for {run_id} never became writable: {err}"
                );
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }
}

fn init_repo_with_workflow(repo_dir: &Path) {
    std::fs::write(repo_dir.join("README.md"), "recovery test\n")
        .expect("recovery README fixture should write");
    std::fs::write(
        repo_dir.join("workflow.fabro"),
        "\
digraph Recovery {
  start [shape=Mdiamond, label=\"Start\"]
  exit  [shape=Msquare, label=\"Exit\"]
  plan  [label=\"Plan\", shape=parallelogram, script=\"echo plan\"]
  build [label=\"Build\", shape=parallelogram, script=\"echo build\"]
  start -> plan -> build -> exit
}
",
    )
    .expect("recovery workflow fixture should write");

    let init = std::process::Command::new("git")
        .args(["init"])
        .current_dir(repo_dir)
        .status()
        .expect("git init should launch");
    assert!(init.success(), "git init should succeed");

    let add = std::process::Command::new("git")
        .args(["add", "README.md", "workflow.fabro"])
        .current_dir(repo_dir)
        .status()
        .expect("git add should launch");
    assert!(add.success(), "git add should succeed");

    let commit = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=Fabro",
            "-c",
            "user.email=noreply@fabro.sh",
            "commit",
            "-m",
            "init",
        ])
        .current_dir(repo_dir)
        .status()
        .expect("git commit should launch");
    assert!(commit.success(), "git commit should succeed");

    let remote_dir = repo_dir
        .parent()
        .expect("recovery repo should have a parent")
        .join(format!(
            "{}-remote.git",
            repo_dir
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("recovery")
        ));
    let remote_init = std::process::Command::new("git")
        .args(["init", "--bare", "-q"])
        .arg(&remote_dir)
        .status()
        .expect("git init --bare should launch");
    assert!(remote_init.success(), "git init --bare should succeed");

    let remote_add = std::process::Command::new("git")
        .args(["remote", "add", "origin"])
        .arg(&remote_dir)
        .current_dir(repo_dir)
        .status()
        .expect("git remote add should launch");
    assert!(remote_add.success(), "git remote add should succeed");

    let push = std::process::Command::new("git")
        .args(["push", "-u", "origin", "HEAD:main"])
        .current_dir(repo_dir)
        .status()
        .expect("git push should launch");
    assert!(push.success(), "git push should succeed");
}

#[test]
fn rewind_list_reports_empty_timeline_when_metadata_branch_is_missing() {
    let context = test_context!();
    context.ensure_home_server_auth_methods();
    let repo_dir = tempfile::tempdir().unwrap();
    let source_run_id = unique_run_id();

    init_repo_with_workflow(repo_dir.path());

    context
        .command()
        .current_dir(repo_dir.path())
        .args([
            "run",
            "--dry-run",
            "--no-retro",
            "--sandbox",
            "local",
            "--run-id",
            source_run_id.as_str(),
            "workflow.fabro",
        ])
        .assert()
        .success();

    delete_metadata_branch_when_ready(repo_dir.path(), &source_run_id);

    assert!(
        list_metadata_run_ids(repo_dir.path()).is_empty(),
        "metadata branch should start missing"
    );

    let mut rewind_list = context.command();
    rewind_list.current_dir(repo_dir.path());
    rewind_list.args(["rewind", &source_run_id, "--list"]);
    rewind_list.timeout(std::time::Duration::from_secs(15));
    fabro_snapshot!(context.filters(), rewind_list, @"
    success: true
    exit_code: 0
    ----- stdout -----
    ----- stderr -----
    @   Node   Details
     @1  start  (no run commit)
     @2  plan
     @3  build
    ");

    assert!(
        list_metadata_run_ids(repo_dir.path()).is_empty(),
        "server timeline should not rebuild missing metadata"
    );
}
