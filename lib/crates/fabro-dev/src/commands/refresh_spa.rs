use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Args;
use walkdir::WalkDir;

use super::check_spa_budgets::check_spa_asset_budgets;
use super::workspace_root;

const DEFAULT_ASSET_BUDGET_BYTES: u64 = 15 * 1024 * 1024;
const DEFAULT_PAYLOAD_BUDGET_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Debug, Args)]
pub(crate) struct RefreshSpaArgs {
    /// Repository root containing apps/fabro-web and lib/crates/fabro-spa.
    #[arg(long, hide = true)]
    root: Option<PathBuf>,
    /// Skip bun run build and only mirror an existing dist directory.
    #[arg(long, hide = true)]
    pub(super) skip_build: bool,
    /// Override the raw asset budget.
    #[arg(long, hide = true, default_value_t = DEFAULT_ASSET_BUDGET_BYTES)]
    pub(super) asset_budget_bytes: u64,
    /// Override the estimated gzip payload budget.
    #[arg(long, hide = true, default_value_t = DEFAULT_PAYLOAD_BUDGET_BYTES)]
    pub(super) payload_budget_bytes: u64,
}

pub(crate) fn refresh_spa(args: RefreshSpaArgs) -> Result<()> {
    let root = args.root.unwrap_or_else(workspace_root);
    refresh_spa_root(
        &root,
        args.skip_build,
        args.asset_budget_bytes,
        args.payload_budget_bytes,
    )
}

#[expect(
    clippy::print_stdout,
    reason = "dev spa refresh command reports progress directly"
)]
pub(super) fn refresh_spa_root(
    root: &Path,
    skip_build: bool,
    asset_budget_bytes: u64,
    payload_budget_bytes: u64,
) -> Result<()> {
    let web_dir = root.join("apps/fabro-web");
    let dist_dir = web_dir.join("dist");
    let asset_dir = root.join("lib/crates/fabro-spa/assets");

    if !skip_build {
        println!("Running bun run build in apps/fabro-web...");
        run_bun_build(&web_dir)?;
    }

    let staging = TempDir::new(root, "refresh")?;
    mirror_dist(&dist_dir, staging.path())?;
    check_spa_asset_budgets(staging.path(), asset_budget_bytes, payload_budget_bytes)?;
    mirror_dist(staging.path(), &asset_dir)?;
    println!("Refreshed lib/crates/fabro-spa/assets");

    Ok(())
}

#[expect(
    clippy::disallowed_methods,
    reason = "dev spa refresh intentionally runs a synchronous Bun subprocess"
)]
pub(super) fn run_bun_build(web_dir: &Path) -> Result<()> {
    let status = Command::new("bun")
        .args(["run", "build"])
        .current_dir(web_dir)
        .status()
        .with_context(|| format!("running bun run build in {}", web_dir.display()))?;
    if !status.success() {
        bail!("bun run build failed with {status}");
    }

    Ok(())
}

#[expect(
    clippy::disallowed_methods,
    reason = "dev spa refresh mirrors build output with synchronous filesystem operations"
)]
pub(super) fn mirror_dist(dist_dir: &Path, asset_dir: &Path) -> Result<()> {
    if !dist_dir.is_dir() {
        bail!("apps/fabro-web/dist is missing; run `bun run build` before mirroring SPA assets");
    }

    if asset_dir.exists() {
        std::fs::remove_dir_all(asset_dir)
            .with_context(|| format!("removing {}", asset_dir.display()))?;
    }
    std::fs::create_dir_all(asset_dir)
        .with_context(|| format!("creating {}", asset_dir.display()))?;

    for entry in WalkDir::new(dist_dir) {
        let entry = entry.context("walking apps/fabro-web/dist")?;
        let source = entry.path();
        let relative = source
            .strip_prefix(dist_dir)
            .with_context(|| format!("{} is not under {}", source.display(), dist_dir.display()))?;
        if relative.as_os_str().is_empty() {
            continue;
        }

        let destination = asset_dir.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&destination)
                .with_context(|| format!("creating {}", destination.display()))?;
            continue;
        }

        if source.extension().and_then(|ext| ext.to_str()) == Some("map") {
            continue;
        }

        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::copy(source, &destination).with_context(|| {
            format!("copying {} to {}", source.display(), destination.display())
        })?;
    }

    Ok(())
}

pub(super) struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub(super) fn new(root: &Path, label: &str) -> Result<Self> {
        let base = root.join("tmp");
        std::fs::create_dir_all(&base).with_context(|| format!("creating {}", base.display()))?;

        for attempt in 0..100 {
            let path = base.join(format!(
                "fabro-dev-spa-{label}-{}-{attempt}",
                std::process::id()
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => {
                    return Err(error).with_context(|| format!("creating {}", path.display()));
                }
            }
        }

        bail!(
            "could not create temporary SPA staging directory under {}",
            base.display()
        )
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
