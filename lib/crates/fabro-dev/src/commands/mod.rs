use anyhow::{Result, bail};
use clap::Args;

mod check_boundary;
mod docker_build;

pub(crate) use check_boundary::{CheckBoundaryArgs, check_boundary};
pub(crate) use docker_build::{DockerBuildArgs, docker_build};

#[derive(Debug, Args)]
pub(crate) struct ReleaseArgs;

#[derive(Debug, Args)]
pub(crate) struct RefreshSpaArgs;

#[derive(Debug, Args)]
pub(crate) struct CheckSpaBudgetsArgs;

pub(crate) fn release(_args: ReleaseArgs) -> Result<()> {
    not_yet_implemented("release")
}

pub(crate) fn refresh_spa(_args: RefreshSpaArgs) -> Result<()> {
    not_yet_implemented("refresh-spa")
}

pub(crate) fn check_spa_budgets(_args: CheckSpaBudgetsArgs) -> Result<()> {
    not_yet_implemented("check-spa-budgets")
}

fn not_yet_implemented(command: &str) -> Result<()> {
    bail!("{command} is not yet implemented")
}
