use clap::Parser;

use crate::cli::Args;
mod cli;

fn main() -> anyhow::Result<()> {
    let args = Args::try_parse()?;
    println!("{:?}", args);

    Ok(())
}
