use std::path::{Path, PathBuf};

mod docker_build;
mod generate_cli_reference;
mod generate_options_reference;
mod release;
mod spa;

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

fn output_text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("command output should be valid utf-8")
}

#[expect(
    clippy::disallowed_methods,
    reason = "integration tests stage temporary fixture files with sync std::fs::write"
)]
fn write_file(root: &Path, path: &str, contents: impl AsRef<[u8]>) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().expect("fixture path should have parent"))
        .expect("creating fixture parent directory");
    std::fs::write(path, contents).expect("writing fixture file");
}

#[expect(
    clippy::disallowed_methods,
    reason = "integration tests inspect fixture files with sync std::fs::read_to_string"
)]
fn read_file(root: &Path, path: &str) -> String {
    std::fs::read_to_string(root.join(path)).expect("reading fixture file")
}

#[test]
fn help_lists_scaffolded_commands() {
    let output = fabro_dev()
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = output_text(&output.stdout);

    for command in [
        "docker-build",
        "generate-cli-reference",
        "generate-options-reference",
        "release",
        "refresh-spa",
        "check-spa-budgets",
    ] {
        assert!(
            stdout.contains(command),
            "top-level help should list {command}:\n{stdout}"
        );
    }
}

#[test]
fn cargo_dev_alias_points_at_fabro_dev() {
    let config = read_file(&workspace_root(), ".cargo/config.toml");
    assert!(
        config.contains(r#"dev = "run --package fabro-dev --""#),
        "cargo dev alias should invoke fabro-dev:\n{config}"
    );
}

#[test]
fn unknown_subcommand_exits_with_clap_usage_error() {
    let output = fabro_dev()
        .arg("not-a-command")
        .assert()
        .failure()
        .code(2)
        .get_output()
        .clone();
    let stderr = output_text(&output.stderr);

    assert!(
        stderr.contains("unrecognized subcommand 'not-a-command'"),
        "unknown subcommand should report clap usage error:\n{stderr}"
    );
}
