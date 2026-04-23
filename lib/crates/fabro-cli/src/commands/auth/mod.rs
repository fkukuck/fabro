mod login;
mod logout;
mod status;

use anyhow::Result;
use fabro_types::settings::cli::CliLayer;
use fabro_util::printer::Printer;

use crate::args::{AuthCommand, AuthNamespace};

pub(crate) async fn dispatch(
    ns: AuthNamespace,
    cli_layer: &CliLayer,
    process_local_json: bool,
    printer: Printer,
) -> Result<()> {
    match ns.command {
        AuthCommand::Login(args) => {
            login::login_command(args, cli_layer, process_local_json, printer).await
        }
        AuthCommand::Logout(args) => {
            logout::logout_command(args, cli_layer, process_local_json, printer).await
        }
        AuthCommand::Status(args) => {
            status::status_command(&args, cli_layer, process_local_json, printer)
        }
    }
}
