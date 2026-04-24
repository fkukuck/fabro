use std::fs;
use std::path::{Path, PathBuf};

fn fabro_dev() -> assert_cmd::Command {
    assert_cmd::cargo::cargo_bin_cmd!("fabro-dev")
}

fn workspace_root() -> PathBuf {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.pop();
    root.pop();
    root.pop();
    root
}

#[expect(
    clippy::disallowed_methods,
    reason = "integration tests stage temporary Rust source fixtures with sync std::fs::write"
)]
fn write_file(root: &Path, path: &str, contents: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("fixture path should have parent"))
        .expect("creating fixture parent directory");
    fs::write(path, contents).expect("writing fixture file");
}

fn check_boundary(root: &Path) -> assert_cmd::Command {
    let mut cmd = fabro_dev();
    cmd.args(["check-boundary", "--root"]).arg(root);
    cmd
}

fn output_text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("command output should be valid utf-8")
}

#[test]
fn current_tree_passes_boundary_check() {
    let output = check_boundary(&workspace_root())
        .assert()
        .success()
        .get_output()
        .clone();

    assert!(
        output_text(&output.stdout).contains("CLI/server boundary checks passed."),
        "success output should report boundary check pass"
    );
}

#[test]
fn empty_workspace_passes() {
    let fixture = tempfile::tempdir().expect("creating fixture");

    check_boundary(fixture.path()).assert().success();
}

#[test]
fn allowed_symbols_pass() {
    let fixture = tempfile::tempdir().expect("creating fixture");
    write_file(
        fixture.path(),
        "lib/crates/fabro-cli/src/local_server.rs",
        "fn resolve() { fabro_config::resolve_server(); }\n",
    );
    write_file(
        fixture.path(),
        "lib/crates/fabro-cli/src/command_context.rs",
        "fn storage() { Storage::new(path); }\n",
    );

    check_boundary(fixture.path()).assert().success();
}

#[test]
fn temporary_exemption_marker_allows_listed_file() {
    let fixture = tempfile::tempdir().expect("creating fixture");
    write_file(
        fixture.path(),
        "lib/crates/fabro-cli/src/commands/pr/mod.rs",
        "// boundary-exempt(pr-api): remove with follow-up #1\nfn storage() { Storage::new(path); }\n",
    );

    check_boundary(fixture.path()).assert().success();
}

#[test]
fn offending_file_reports_path_and_line() {
    let fixture = tempfile::tempdir().expect("creating fixture");
    write_file(
        fixture.path(),
        "lib/crates/fabro-cli/src/commands/bad.rs",
        "fn storage() { Storage::new(path); }\n",
    );

    let output = check_boundary(fixture.path())
        .assert()
        .failure()
        .code(1)
        .get_output()
        .clone();
    let stderr = output_text(&output.stderr);

    assert!(
        stderr.contains(
            "boundary check failed: Storage::new used outside allowlist: \
             lib/crates/fabro-cli/src/commands/bad.rs:1"
        ),
        "stderr should name offending file and line:\n{stderr}"
    );
}

#[test]
fn multiple_offending_files_are_all_reported() {
    let fixture = tempfile::tempdir().expect("creating fixture");
    write_file(
        fixture.path(),
        "lib/crates/fabro-cli/src/commands/bad_storage.rs",
        "fn storage() { Storage::new(path); }\n",
    );
    write_file(
        fixture.path(),
        "lib/crates/fabro-cli/src/commands/bad_server.rs",
        "fn server() { ServerSettings::resolve(layer); }\n",
    );

    let output = check_boundary(fixture.path())
        .assert()
        .failure()
        .code(1)
        .get_output()
        .clone();
    let stderr = output_text(&output.stderr);

    assert!(
        stderr.contains("bad_storage.rs:1"),
        "stderr should report storage offender:\n{stderr}"
    );
    assert!(
        stderr.contains("bad_server.rs:1"),
        "stderr should report server-settings offender:\n{stderr}"
    );
}

#[test]
fn unexpected_temporary_exemption_marker_fails() {
    let fixture = tempfile::tempdir().expect("creating fixture");
    write_file(
        fixture.path(),
        "lib/crates/fabro-cli/src/commands/bad.rs",
        "// boundary-exempt(pr-api): remove with follow-up #1\n",
    );

    let output = check_boundary(fixture.path())
        .assert()
        .failure()
        .code(1)
        .get_output()
        .clone();
    let stderr = output_text(&output.stderr);

    assert!(
        stderr.contains(
            "boundary check failed: unexpected temporary exemption marker in \
             lib/crates/fabro-cli/src/commands/bad.rs:1"
        ),
        "stderr should report unexpected marker:\n{stderr}"
    );
}
