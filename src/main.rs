use clap::Parser;
use tokio::fs::read;

use crate::cli::Args;
mod cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::try_parse()?;
    println!("{:?}", args);

    let config = read(args.config_dir.join("executors.toml")).await?;

    println!(
        "Config:\n{:#?}",
        String::from_utf8_lossy(&config).parse::<toml::Value>()
    );

    Ok(())
}
