use std::path::Path;

use super::{fabro_dev, output_text, read_file, write_file};

fn cli_reference(root: &Path) -> assert_cmd::Command {
    let mut cmd = fabro_dev();
    cmd.args(["generate-cli-reference", "--root"]).arg(root);
    cmd
}

#[test]
fn write_updates_only_generated_region() {
    let fixture = tempfile::tempdir().expect("creating fixture");
    write_file(
        fixture.path(),
        "docs/reference/cli.mdx",
        r"---
title: CLI
---

Intro copy.

<!-- generated:cli -->
stale
<!-- /generated:cli -->

Tail copy.
",
    );

    cli_reference(fixture.path()).assert().success();

    let contents = read_file(fixture.path(), "docs/reference/cli.mdx");
    assert!(
        contents.contains("Intro copy."),
        "manual intro should be preserved:\n{contents}"
    );
    assert!(
        contents.contains("Tail copy."),
        "manual tail should be preserved:\n{contents}"
    );
    assert!(
        contents.contains("## `fabro`"),
        "generated output should include root command reference:\n{contents}"
    );
    assert!(
        contents.contains("### `fabro run`"),
        "generated output should include subcommand reference:\n{contents}"
    );
    assert!(
        !contents.contains("stale"),
        "stale generated content should be replaced:\n{contents}"
    );
}

#[test]
fn check_passes_after_write() {
    let fixture = tempfile::tempdir().expect("creating fixture");
    write_file(
        fixture.path(),
        "docs/reference/cli.mdx",
        r"<!-- generated:cli -->
stale
<!-- /generated:cli -->
",
    );

    cli_reference(fixture.path()).assert().success();

    cli_reference(fixture.path())
        .arg("--check")
        .assert()
        .success();
}

#[test]
fn check_fails_when_generated_region_is_stale() {
    let fixture = tempfile::tempdir().expect("creating fixture");
    write_file(
        fixture.path(),
        "docs/reference/cli.mdx",
        r"<!-- generated:cli -->
stale
<!-- /generated:cli -->
",
    );

    let output = cli_reference(fixture.path())
        .arg("--check")
        .assert()
        .failure()
        .code(1)
        .get_output()
        .clone();
    let stderr = output_text(&output.stderr);

    assert!(
        stderr.contains("docs/reference/cli.mdx is stale; run `cargo dev generate-cli-reference`"),
        "check failure should explain how to regenerate:\n{stderr}"
    );
}

#[test]
fn generated_reference_is_deterministic() {
    let fixture = tempfile::tempdir().expect("creating fixture");
    write_file(
        fixture.path(),
        "docs/reference/cli.mdx",
        r"<!-- generated:cli -->
stale
<!-- /generated:cli -->
",
    );

    cli_reference(fixture.path()).assert().success();
    let first = read_file(fixture.path(), "docs/reference/cli.mdx");

    cli_reference(fixture.path()).assert().success();
    let second = read_file(fixture.path(), "docs/reference/cli.mdx");

    assert_eq!(first, second);
}
