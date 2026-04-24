use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, Command};

const CLI_REFERENCE_PATH: &str = "docs/reference/cli.mdx";
const FENCE_START: &str = "<!-- generated:cli -->";
const FENCE_END: &str = "<!-- /generated:cli -->";

#[derive(Debug, clap::Args)]
pub(crate) struct GenerateCliReferenceArgs {
    /// Verify docs/reference/cli.mdx is up to date without rewriting it.
    #[arg(long)]
    check: bool,
    /// Workspace root containing docs/reference/cli.mdx.
    #[arg(long, hide = true)]
    root:  Option<PathBuf>,
}

#[expect(
    clippy::print_stdout,
    clippy::disallowed_methods,
    reason = "dev generator reports the generated docs path directly and intentionally uses sync filesystem I/O"
)]
pub(crate) fn generate_cli_reference(args: GenerateCliReferenceArgs) -> Result<()> {
    let root = args.root.unwrap_or_else(workspace_root);
    let path = root.join(CLI_REFERENCE_PATH);
    let current =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let generated = render_cli_reference(fabro_cli::command_for_reference());
    let updated = replace_generated_region(&current, &generated)?;

    if args.check {
        if current != updated {
            bail!("{CLI_REFERENCE_PATH} is stale; run `cargo dev generate-cli-reference`");
        }
        println!("{CLI_REFERENCE_PATH} is up to date.");
        return Ok(());
    }

    if current != updated {
        std::fs::write(&path, updated).with_context(|| format!("writing {}", path.display()))?;
    }
    println!("Generated {CLI_REFERENCE_PATH}.");
    Ok(())
}

fn render_cli_reference(mut command: Command) -> String {
    command.build();

    let mut output = String::new();
    render_command(&mut output, &command, &[], 2);
    output.trim_end().to_string()
}

fn render_command(output: &mut String, command: &Command, parents: &[&str], level: usize) {
    if command.is_hide_set() {
        return;
    }

    let path = command_path(command, parents);
    output.push_str(&"#".repeat(level));
    output.push_str(" `");
    output.push_str(&path);
    output.push_str("`\n\n");

    if let Some(about) = command_about(command) {
        output.push_str(&about);
        output.push_str("\n\n");
    }

    output.push_str("```bash\n");
    output.push_str(&usage(command));
    output.push_str("\n```\n\n");

    let positionals = visible_positionals(command);
    if !positionals.is_empty() {
        output.push_str("#### Arguments\n\n");
        output.push_str("| Name | Description |\n");
        output.push_str("| --- | --- |\n");
        for arg in positionals {
            output.push_str("| `");
            output.push_str(&argument_name(arg));
            output.push_str("` | ");
            output.push_str(&arg_help(arg));
            output.push_str(" |\n");
        }
        output.push('\n');
    }

    let options = visible_options(command);
    if !options.is_empty() {
        output.push_str("#### Options\n\n");
        output.push_str("| Option | Description |\n");
        output.push_str("| --- | --- |\n");
        for arg in options {
            output.push_str("| `");
            output.push_str(&option_name(arg));
            output.push_str("` | ");
            output.push_str(&arg_help(arg));
            output.push_str(" |\n");
        }
        output.push('\n');
    }

    let mut visible_subcommands = command
        .get_subcommands()
        .filter(|subcommand| !subcommand.is_hide_set() && subcommand.get_name() != "help")
        .collect::<Vec<_>>();
    visible_subcommands.sort_by_key(|subcommand| subcommand.get_name());

    if !visible_subcommands.is_empty() {
        output.push_str("#### Subcommands\n\n");
        output.push_str("| Command | Description |\n");
        output.push_str("| --- | --- |\n");
        for subcommand in &visible_subcommands {
            output.push_str("| `");
            output.push_str(&command_path(subcommand, &[&path]));
            output.push_str("` | ");
            output.push_str(&command_about(subcommand).unwrap_or_default());
            output.push_str(" |\n");
        }
        output.push('\n');
    }

    let mut next_parents = parents.to_vec();
    next_parents.push(command.get_name());
    for subcommand in visible_subcommands {
        render_command(output, subcommand, &next_parents, level + 1);
    }
}

fn command_path(command: &Command, parents: &[&str]) -> String {
    parents
        .iter()
        .copied()
        .chain([command.get_name()])
        .collect::<Vec<_>>()
        .join(" ")
}

fn usage(command: &Command) -> String {
    let mut command = command.clone();
    let usage = command.render_usage().to_string();
    usage.trim_start_matches("Usage: ").trim().to_string()
}

fn command_about(command: &Command) -> Option<String> {
    command
        .get_long_about()
        .or_else(|| command.get_about())
        .map(ToString::to_string)
        .map(|help| markdown_cell(help.trim()))
        .filter(|help| !help.is_empty())
}

fn visible_positionals(command: &Command) -> Vec<&Arg> {
    command
        .get_positionals()
        .filter(|arg| !arg.is_hide_set())
        .collect()
}

fn visible_options(command: &Command) -> Vec<&Arg> {
    let mut options = command
        .get_arguments()
        .filter(|arg| {
            !arg.is_positional()
                && !arg.is_hide_set()
                && !arg.is_global_set()
                && !matches!(arg.get_id().as_str(), "help" | "version")
        })
        .collect::<Vec<_>>();
    options.sort_by_key(|arg| arg.get_id().to_string());
    options
}

fn argument_name(arg: &Arg) -> String {
    arg.get_value_names()
        .and_then(|names| names.first())
        .map_or_else(|| arg.get_id().to_string(), ToString::to_string)
}

fn option_name(arg: &Arg) -> String {
    let mut names = Vec::new();
    if let Some(short) = arg.get_short() {
        names.push(format!("-{short}"));
    }
    if let Some(long) = arg.get_long() {
        names.push(format!("--{long}"));
    }
    if names.is_empty() {
        names.push(arg.get_id().to_string());
    }

    let mut name = names.join(", ");
    if option_takes_value(arg) {
        name.push(' ');
        name.push('<');
        name.push_str(&argument_name(arg).to_ascii_lowercase());
        name.push('>');
    }
    name
}

fn option_takes_value(arg: &Arg) -> bool {
    arg.get_num_args().is_some_and(|range| range.takes_values())
}

fn arg_help(arg: &Arg) -> String {
    let mut parts = Vec::new();
    if let Some(help) = arg.get_long_help().or_else(|| arg.get_help()) {
        let help = help.to_string();
        let help = help.trim();
        if !help.is_empty() {
            parts.push(markdown_cell(help));
        }
    }

    let possible_values = arg
        .get_possible_values()
        .into_iter()
        .filter(|value| !value.is_hide_set())
        .map(|value| format!("`{}`", value.get_name()))
        .collect::<Vec<_>>();
    if !possible_values.is_empty() {
        parts.push(format!("Values: {}", possible_values.join(", ")));
    }

    if !is_boolean_switch(arg) {
        let defaults = arg
            .get_default_values()
            .iter()
            .filter_map(|value| os_str_to_markdown_code(value.as_os_str()))
            .collect::<Vec<_>>();
        if !defaults.is_empty() {
            parts.push(format!("Default: {}", defaults.join(", ")));
        }
    }

    if parts.is_empty() {
        "TODO: add CLI help text.".to_string()
    } else {
        parts.join("<br />")
    }
}

fn is_boolean_switch(arg: &Arg) -> bool {
    matches!(arg.get_action(), ArgAction::SetTrue | ArgAction::SetFalse)
}

fn os_str_to_markdown_code(value: &OsStr) -> Option<String> {
    let value = value.to_str()?;
    (!value.is_empty()).then(|| format!("`{}`", markdown_cell(value)))
}

fn markdown_cell(value: &str) -> String {
    value
        .replace('|', "\\|")
        .replace('\n', "<br />")
        .trim()
        .to_string()
}

fn replace_generated_region(current: &str, generated: &str) -> Result<String> {
    let start = current
        .find(FENCE_START)
        .with_context(|| format!("{CLI_REFERENCE_PATH} is missing {FENCE_START}"))?;
    let content_start = start + FENCE_START.len();
    let relative_end = current[content_start..]
        .find(FENCE_END)
        .with_context(|| format!("{CLI_REFERENCE_PATH} is missing {FENCE_END}"))?;
    let end = content_start + relative_end;

    let before = &current[..content_start];
    let after = &current[end..];
    Ok(format!("{before}\n{generated}\n{after}"))
}

fn workspace_root() -> PathBuf {
    let mut root = Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf();
    root.pop();
    root.pop();
    root.pop();
    root
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_generated_region_preserves_manual_content() {
        let updated = replace_generated_region(
            "before\n<!-- generated:cli -->\nstale\n<!-- /generated:cli -->\nafter\n",
            "fresh",
        )
        .expect("generated region should be replaced");

        assert_eq!(
            updated,
            "before\n<!-- generated:cli -->\nfresh\n<!-- /generated:cli -->\nafter\n"
        );
    }
}
