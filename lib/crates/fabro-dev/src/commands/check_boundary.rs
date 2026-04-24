use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Args;
use regex::Regex;
use walkdir::WalkDir;

const FABRO_CLI_SRC: &str = "lib/crates/fabro-cli/src";
const EXEMPTION_MARKER: &str = "boundary-exempt(pr-api): remove with follow-up #1";

const SERVER_SYMBOL_ALLOWLIST: &[&str] = &[
    "lib/crates/fabro-cli/src/local_server.rs",
    "lib/crates/fabro-cli/src/commands/install.rs",
    "lib/crates/fabro-cli/src/commands/run/runner.rs",
];

const STORAGE_ALLOWLIST: &[&str] = &[
    "lib/crates/fabro-cli/src/command_context.rs",
    "lib/crates/fabro-cli/src/commands/install.rs",
    "lib/crates/fabro-cli/src/commands/uninstall.rs",
    "lib/crates/fabro-cli/src/commands/run/runner.rs",
];

const DEPRECATED_HELPER_ALLOWLIST: &[&str] = &["lib/crates/fabro-cli/src/user_config.rs"];

const TEMPORARY_EXEMPTIONS: &[&str] = &[
    "lib/crates/fabro-cli/src/commands/pr/mod.rs",
    "lib/crates/fabro-cli/src/commands/pr/create.rs",
];

#[derive(Debug, Args)]
pub(crate) struct CheckBoundaryArgs {
    /// Workspace root to check.
    #[arg(long, value_name = "ROOT", default_value = ".")]
    root: PathBuf,
}

struct BoundaryRule {
    name:      &'static str,
    pattern:   Regex,
    allowlist: &'static [&'static str],
}

struct SourceFile {
    relative: String,
    contents: String,
}

#[expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "dev check command reports pass/fail diagnostics directly"
)]
pub(crate) fn check_boundary(args: CheckBoundaryArgs) -> Result<()> {
    let failures = BoundaryChecker::new(args.root).check()?;

    if failures.is_empty() {
        println!("CLI/server boundary checks passed.");
        return Ok(());
    }

    for failure in failures {
        eprintln!("{failure}");
    }
    bail!("CLI/server boundary checks failed")
}

struct BoundaryChecker {
    root: PathBuf,
}

impl BoundaryChecker {
    fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn check(&self) -> Result<Vec<String>> {
        let rules = boundary_rules();
        let mut failures = Vec::new();

        for file in self.source_files()? {
            let has_marker = marker_lines(&file.contents).next().is_some();
            let has_valid_exemption =
                has_marker && TEMPORARY_EXEMPTIONS.contains(&file.relative.as_str());

            for line_number in marker_lines(&file.contents) {
                if !TEMPORARY_EXEMPTIONS.contains(&file.relative.as_str()) {
                    failures.push(format!(
                        "boundary check failed: unexpected temporary exemption marker in {}:{}",
                        file.relative, line_number
                    ));
                }
            }

            for rule in &rules {
                if rule.allowlist.contains(&file.relative.as_str()) || has_valid_exemption {
                    continue;
                }

                for (line_number, line) in file.contents.lines().enumerate() {
                    if rule.pattern.is_match(line) {
                        failures.push(format!(
                            "boundary check failed: {} used outside allowlist: {}:{}",
                            rule.name,
                            file.relative,
                            line_number + 1
                        ));
                    }
                }
            }
        }

        Ok(failures)
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "dev boundary checker synchronously scans source files outside Tokio paths"
    )]
    fn source_files(&self) -> Result<Vec<SourceFile>> {
        let source_root = self.root.join(FABRO_CLI_SRC);
        if !source_root.exists() {
            return Ok(Vec::new());
        }

        let mut files = Vec::new();
        for entry in WalkDir::new(&source_root) {
            let entry = entry.context("walking fabro-cli source tree")?;
            let path = entry.path();
            if !entry.file_type().is_file()
                || path.extension().and_then(|ext| ext.to_str()) != Some("rs")
            {
                continue;
            }

            let contents = fs::read_to_string(path)
                .with_context(|| format!("reading source file {}", path.display()))?;
            files.push(SourceFile {
                relative: relative_path(&self.root, path)?,
                contents,
            });
        }

        files.sort_by(|left, right| left.relative.cmp(&right.relative));
        Ok(files)
    }
}

fn boundary_rules() -> [BoundaryRule; 3] {
    [
        BoundaryRule {
            name:      "gated server symbol",
            pattern:   Regex::new(
                r"fabro_config::resolve_server_from_file|fabro_config::resolve_server\b|fabro_config::ServerSettings::from_layer\b|fabro_config::ServerSettings::resolve\b|ServerSettings::from_layer\b|ServerSettings::resolve\b",
            )
            .expect("server symbol regex should compile"),
            allowlist: SERVER_SYMBOL_ALLOWLIST,
        },
        BoundaryRule {
            name:      "Storage::new",
            pattern:   Regex::new(r"Storage::new").expect("storage regex should compile"),
            allowlist: STORAGE_ALLOWLIST,
        },
        BoundaryRule {
            name:      "deprecated user_config::storage_dir",
            pattern:   Regex::new(r"user_config::storage_dir")
                .expect("deprecated helper regex should compile"),
            allowlist: DEPRECATED_HELPER_ALLOWLIST,
        },
    ]
}

fn marker_lines(contents: &str) -> impl Iterator<Item = usize> + '_ {
    contents.lines().enumerate().filter_map(|(index, line)| {
        if line.contains(EXEMPTION_MARKER) {
            Some(index + 1)
        } else {
            None
        }
    })
}

fn relative_path(root: &Path, path: &Path) -> Result<String> {
    let relative = path
        .strip_prefix(root)
        .with_context(|| format!("{} is not under {}", path.display(), root.display()))?;

    Ok(relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/"))
}
