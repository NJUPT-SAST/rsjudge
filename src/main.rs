use clap::Parser;
use env_logger::Env;
use log::{debug, info};
use tokio::fs::read;

use crate::cli::Args;
mod cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(
        Env::new()
            .filter_or("RSJUDGE_LOG", "info")
            .write_style("RSJUDGE_LOG_STYLE"),
    )
    .try_init()?;

    let args = Args::try_parse()?;
    info!("{:?}", args);

    let config = read(args.config_dir.join("executors.toml")).await?;

    debug!(
        "Config:\n{:#?}",
        String::from_utf8_lossy(&config).parse::<toml::Value>()
    );

    Ok(())
}
