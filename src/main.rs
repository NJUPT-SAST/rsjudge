use clap::Parser;
use toml::Value;

use crate::cli::Args;
mod cli;

fn main() -> anyhow::Result<()> {
    let args = Args::try_parse()?;
    println!("{:?}", args);

    println!(
        "Config:\n{:#?}",
        toml::from_str::<Value>(include_str!("../templates/executors.toml"))?
    );

    Ok(())
}
