use anyhow::{Context, Result};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub fn init_tracing(debug: bool) -> Result<()> {
    let default_level = if debug { "debug" } else { "info" };
    let filter =
        EnvFilter::try_from_env("ARC_LOG").unwrap_or_else(|_| EnvFilter::new(default_level));

    let log_dir = dirs::home_dir()
        .map(|h| h.join(".arc").join("logs"))
        .unwrap_or_else(|| ".arc/logs".into());

    std::fs::create_dir_all(&log_dir)
        .with_context(|| format!("Failed to create log directory: {}", log_dir.display()))?;

    let filename = chrono::Local::now().format("%Y-%m-%d.log").to_string();
    let file_appender = tracing_appender::rolling::never(&log_dir, &filename);

    tracing_subscriber::registry()
        .with(filter)
        .with(
            fmt::layer()
                .with_writer(file_appender)
                .with_target(true)
                .with_ansi(false),
        )
        .init();

    Ok(())
}
