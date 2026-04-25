use anyhow::Result;
use clap::{Args, Subcommand};

use super::check_spa_budgets::{CheckSpaBudgetsArgs, check_spa_budgets};
use super::refresh_spa::{RefreshSpaArgs, refresh_spa};

#[derive(Debug, Args)]
pub(crate) struct SpaArgs {
    #[command(subcommand)]
    command: Option<SpaCommand>,
}

#[derive(Debug, Subcommand)]
enum SpaCommand {
    /// Rebuild and refresh the embedded Fabro web SPA bundle.
    Refresh(RefreshSpaArgs),
    /// Verify embedded Fabro web SPA assets are current and within budget.
    Check(CheckSpaBudgetsArgs),
}

pub(crate) fn spa(args: SpaArgs) -> Result<()> {
    match args.command {
        Some(SpaCommand::Refresh(args)) => refresh_spa(args),
        Some(SpaCommand::Check(args)) => check_spa_budgets(args),
        None => print_spa_help(),
    }
}

#[expect(
    clippy::print_stdout,
    reason = "dev spa command prints group help for non-mutating discovery"
)]
fn print_spa_help() -> Result<()> {
    let mut command = SpaCommand::augment_subcommands(
        clap::Command::new("spa")
            .about("Manage embedded Fabro web SPA assets")
            .override_usage("fabro-dev spa <COMMAND>"),
    );
    command.print_help()?;
    println!();
    Ok(())
}
