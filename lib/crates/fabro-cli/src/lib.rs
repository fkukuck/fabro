#![expect(
    dead_code,
    reason = "the reference-facing library compiles CLI args without the binary dispatch modules"
)]

mod args;

use clap::{Command, CommandFactory};

pub fn command_for_reference() -> Command {
    args::Cli::command()
}
