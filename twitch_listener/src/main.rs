#![warn(clippy::unwrap_in_result)]
pub mod opts;
pub mod util;
pub mod websocket;

use clap::Parser;
pub use opts::Secret;

use opts::Opts;

#[tokio::main]
async fn main() -> Result<(), eyre::Report> {
    util::install_utils()?;
    let opts = Opts::parse();

    tracing::debug!(
        "App started!\n{}",
        Opts::try_parse_from(["app", "--version"])
            .unwrap_err()
            .to_string()
    );

    tracing::debug!(opts = ?opts);

    Ok(())
}
