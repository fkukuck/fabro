use fabro_test::{fabro_snapshot, test_context};

#[test]
fn help() {
    let context = test_context!();
    let mut cmd = context.command();
    cmd.args(["auth", "--help"]);
    fabro_snapshot!(context.filters(), cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----
    Manage CLI authentication state

    Usage: fabro auth [OPTIONS] <COMMAND>

    Commands:
      login   Log in to a Fabro server
      logout  Log out from a Fabro server
      status  Show offline CLI auth status
      help    Print this message or the help of the given subcommand(s)

    Options:
          --json              Output as JSON [env: FABRO_JSON=]
          --debug             Enable DEBUG-level logging (default is INFO) [env: FABRO_DEBUG=]
          --no-upgrade-check  Disable automatic upgrade check [env: FABRO_NO_UPGRADE_CHECK=true]
          --quiet             Suppress non-essential output [env: FABRO_QUIET=]
          --verbose           Enable verbose output [env: FABRO_VERBOSE=]
      -h, --help              Print help
    ----- stderr -----
    ");
}

#[test]
fn login_help() {
    let context = test_context!();
    let mut cmd = context.command();
    cmd.args(["auth", "login", "--help"]);
    fabro_snapshot!(context.filters(), cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----
    Log in to a Fabro server

    Usage: fabro auth login [OPTIONS]

    Options:
          --json               Output as JSON [env: FABRO_JSON=]
          --server <SERVER>    Fabro server target: http(s) URL or absolute Unix socket path [env: FABRO_SERVER=]
          --debug              Enable DEBUG-level logging (default is INFO) [env: FABRO_DEBUG=]
          --no-browser         Print the browser URL instead of opening it automatically
          --no-upgrade-check   Disable automatic upgrade check [env: FABRO_NO_UPGRADE_CHECK=true]
          --timeout <TIMEOUT>  Timeout in seconds waiting for the browser flow to complete [default: 300]
          --quiet              Suppress non-essential output [env: FABRO_QUIET=]
          --verbose            Enable verbose output [env: FABRO_VERBOSE=]
      -h, --help               Print help
    ----- stderr -----
    ");
}

#[test]
fn status_help() {
    let context = test_context!();
    let mut cmd = context.command();
    cmd.args(["auth", "status", "--help"]);
    fabro_snapshot!(context.filters(), cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----
    Show offline CLI auth status

    Usage: fabro auth status [OPTIONS]

    Options:
          --json              Output as JSON [env: FABRO_JSON=]
          --server <SERVER>   Fabro server target: http(s) URL or absolute Unix socket path [env: FABRO_SERVER=]
          --debug             Enable DEBUG-level logging (default is INFO) [env: FABRO_DEBUG=]
          --no-upgrade-check  Disable automatic upgrade check [env: FABRO_NO_UPGRADE_CHECK=true]
          --quiet             Suppress non-essential output [env: FABRO_QUIET=]
          --verbose           Enable verbose output [env: FABRO_VERBOSE=]
      -h, --help              Print help
    ----- stderr -----
    ");
}
