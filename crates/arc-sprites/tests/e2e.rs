//! E2E tests against the live Sprites service.
//!
//! Requires an authenticated `sprite` CLI. Run with:
//!   cargo test -p arc-sprites --test e2e -- --ignored

use arc_agent::sandbox::Sandbox;
use arc_sprites::{CliSpriteRunner, SpritesConfig, SpritesSandbox};

/// Full lifecycle test: create sprite, run operations, destroy sprite.
#[tokio::test]
#[ignore]
async fn full_lifecycle() {
    let runner = CliSpriteRunner::new();
    let config = SpritesConfig::default();
    let sandbox = SpritesSandbox::new(Box::new(runner), config);

    // --- Initialize ---
    sandbox.initialize().await.unwrap();
    let name = sandbox.sandbox_info();
    assert!(
        name.starts_with("arc-"),
        "expected sprite name starting with arc-, got: {name}",
    );

    // Wrap the rest in a closure-like block so we can always cleanup
    let result = run_operations(&sandbox).await;

    // --- Cleanup (always runs) ---
    sandbox.cleanup().await.unwrap();

    // Propagate any error from operations
    result.unwrap();
}

async fn run_operations(sandbox: &SpritesSandbox) -> Result<(), String> {
    // --- Metadata ---
    assert_eq!(sandbox.working_directory(), "/home/sprite");
    assert_eq!(sandbox.platform(), "linux");
    assert_eq!(sandbox.os_version(), "Linux (Sprites)");

    // --- exec_command: basic ---
    let result = sandbox
        .exec_command("echo hello", 30_000, None, None, None)
        .await?;
    assert_eq!(result.exit_code, 0, "exec_command failed: {}", result.stderr);
    assert_eq!(result.stdout.trim(), "hello");
    assert!(!result.timed_out);

    // --- exec_command: working directory ---
    let result = sandbox
        .exec_command("pwd", 30_000, Some("/tmp"), None, None)
        .await?;
    assert_eq!(result.exit_code, 0, "pwd failed: {}", result.stderr);
    assert_eq!(result.stdout.trim(), "/tmp");

    // --- exec_command: env vars ---
    let mut env = std::collections::HashMap::new();
    env.insert("TEST_VAR".to_string(), "sprite_value".to_string());
    let result = sandbox
        .exec_command("echo $TEST_VAR", 30_000, None, Some(&env), None)
        .await?;
    assert_eq!(result.exit_code, 0, "env exec failed: {}", result.stderr);
    assert_eq!(result.stdout.trim(), "sprite_value");

    // --- write_file + read_file round-trip ---
    sandbox
        .write_file("test-e2e/hello.txt", "Hello, Sprites!\nSecond line\n")
        .await?;

    let content = sandbox
        .read_file("test-e2e/hello.txt", None, None)
        .await?;
    assert!(
        content.contains("Hello, Sprites!"),
        "read_file missing content: {content}",
    );
    assert!(
        content.contains("1 | "),
        "read_file missing line numbers: {content}",
    );
    assert!(
        content.contains("2 | Second line"),
        "read_file missing second line: {content}",
    );

    // --- read_file with offset and limit ---
    let content = sandbox
        .read_file("test-e2e/hello.txt", Some(1), Some(1))
        .await?;
    assert!(
        content.contains("2 | Second line"),
        "offset read missing line 2: {content}",
    );
    assert!(
        !content.contains("Hello, Sprites!"),
        "offset read should skip line 1: {content}",
    );

    // --- file_exists ---
    assert!(
        sandbox.file_exists("test-e2e/hello.txt").await?,
        "file should exist",
    );
    assert!(
        !sandbox.file_exists("test-e2e/nonexistent.txt").await?,
        "file should not exist",
    );

    // --- delete_file ---
    sandbox
        .write_file("test-e2e/to-delete.txt", "delete me")
        .await?;
    assert!(sandbox.file_exists("test-e2e/to-delete.txt").await?);
    sandbox.delete_file("test-e2e/to-delete.txt").await?;
    assert!(
        !sandbox.file_exists("test-e2e/to-delete.txt").await?,
        "file should be deleted",
    );

    // --- list_directory ---
    sandbox
        .write_file("test-e2e/sub/a.txt", "aaa")
        .await?;
    sandbox
        .write_file("test-e2e/sub/b.txt", "bbb")
        .await?;
    let entries = sandbox.list_directory("test-e2e/sub", None).await?;
    assert_eq!(entries.len(), 2, "expected 2 entries, got: {entries:?}");
    assert_eq!(entries[0].name, "a.txt");
    assert_eq!(entries[1].name, "b.txt");
    assert!(!entries[0].is_dir);

    // --- grep ---
    sandbox
        .write_file("test-e2e/search/code.rs", "fn main() {\n    println!(\"hello\");\n}\n")
        .await?;
    sandbox
        .write_file("test-e2e/search/data.txt", "no match here\n")
        .await?;
    let grep_results = sandbox
        .grep("println", "test-e2e/search", &Default::default())
        .await?;
    assert_eq!(
        grep_results.len(),
        1,
        "expected 1 grep match, got: {grep_results:?}",
    );
    assert!(
        grep_results[0].contains("println"),
        "grep result should contain match: {}",
        grep_results[0],
    );

    // --- grep: no matches ---
    let grep_results = sandbox
        .grep("zzz_no_match_zzz", "test-e2e/search", &Default::default())
        .await?;
    assert!(
        grep_results.is_empty(),
        "expected no grep matches, got: {grep_results:?}",
    );

    // --- glob ---
    let glob_results = sandbox.glob("*.rs", Some("test-e2e/search")).await?;
    assert_eq!(
        glob_results.len(),
        1,
        "expected 1 glob match, got: {glob_results:?}",
    );
    assert!(
        glob_results[0].contains("code.rs"),
        "glob result should contain code.rs: {}",
        glob_results[0],
    );

    // --- download_file_to_local ---
    let download_content = "binary-like content for download test";
    sandbox
        .write_file("test-e2e/download.bin", download_content)
        .await?;

    let tmp = tempfile::tempdir().map_err(|e| format!("tempdir: {e}"))?;
    let local_path = tmp.path().join("downloaded.bin");
    sandbox
        .download_file_to_local("test-e2e/download.bin", &local_path)
        .await?;

    let downloaded = tokio::fs::read_to_string(&local_path)
        .await
        .map_err(|e| format!("read local: {e}"))?;
    assert_eq!(
        downloaded, download_content,
        "download content mismatch",
    );

    Ok(())
}
