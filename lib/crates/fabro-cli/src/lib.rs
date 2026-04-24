#![expect(
    dead_code,
    reason = "the reference-facing library compiles CLI args without the binary dispatch modules"
)]

mod args;

use args::{Commands, GlobalArgs, LONG_VERSION};
use clap::{Command, CommandFactory, Parser};

#[derive(Parser)]
#[command(name = "fabro", version, long_version = LONG_VERSION)]
struct Cli {
    #[command(flatten)]
    globals: GlobalArgs,

    #[command(subcommand)]
    command: Option<Box<Commands>>,
}

pub fn command_for_reference() -> Command {
    Cli::command()
}
