use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::Context;
use tracing::Level;

pub fn install_utils() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv(); //ignore error
    install_tracing().context("install tracing")?;
    Ok(())
}

/// Log file path: `BRUCE_LOG`, then `NULL_TWITCH_LOG`, else `bruce.log`.
/// Logs go to stderr and to this file (append mode); use the file when a TUI owns the terminal.
fn install_tracing() -> anyhow::Result<()> {
    use tracing_error::ErrorLayer;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

    let log_path: PathBuf = std::env::var_os("BRUCE_LOG")
        .or_else(|| std::env::var_os("NULL_TWITCH_LOG"))
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("bruce.log"));

    if let Some(dir) = log_path.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("create log dir {}", dir.display()))?;
        }
    }

    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("open log file {}", log_path.display()))?;

    let fmt_file = fmt::layer()
        .with_writer(Mutex::new(log_file))
        .with_file(true)
        .with_line_number(true)
        .with_target(true)
        .with_ansi(false);

    let mut filter_layer =
        EnvFilter::try_from_default_env().or_else(|_| EnvFilter::try_new("info"))?;
    for directive in [
        "hyper=error",
        "h2=error",
        "rustls=error",
        "tungstenite=error",
        "want=info",
        "reqwest=info",
        "mio=info",
    ] {
        filter_layer = filter_layer.add_directive(directive.parse()?);
    }

    filter_layer = filter_layer.add_directive(Level::DEBUG.into());

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_file)
        .with(ErrorLayer::default())
        .try_init()
        .map_err(|error| anyhow::anyhow!("initialize tracing: {error}"))?;

    tracing::info!(path = %log_path.display(), "tracing file sink enabled");
    Ok(())
}
