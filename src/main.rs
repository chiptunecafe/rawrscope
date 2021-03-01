#![windows_subsystem = "console"]

use anyhow::{anyhow, Context, Result};
use argh::FromArgs;

mod app;

#[derive(Debug, FromArgs)]
/// High performance oscilloscope generation for everyone
pub struct Args {
    /// project to load
    #[argh(positional)]
    project_file: Option<std::path::PathBuf>,
}

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init()
        .map_err(|e| anyhow!(e))
        .context("Failed to initialize `tracing` logging")?;

    // Parse arguments
    let args: Args = argh::from_env();
    tracing::trace!("parsed arguments: {:#?}", args);

    // Initialize and start program
    let a = app::App::new(&args)?;
    a.run();
}
