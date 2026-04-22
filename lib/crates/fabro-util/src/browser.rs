/// When this environment variable is set to any value, [`try_open`] returns
/// `Ok(())` without launching a browser. Test harnesses set it so spawned
/// `fabro` subprocesses do not pop real browser windows during CI or local
/// runs.
pub const SUPPRESS_ENV_VAR: &str = "FABRO_SUPPRESS_OPEN_BROWSER";

pub fn try_open(url: &str) -> std::io::Result<()> {
    if std::env::var_os(SUPPRESS_ENV_VAR).is_some() {
        return Ok(());
    }
    open::that(url)
}
