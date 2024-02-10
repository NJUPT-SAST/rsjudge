use clap::Parser;

use crate::cli::Args;

mod cli;

/// For future use.
pub mod user;

fn main() -> anyhow::Result<()> {
    let args = Args::try_parse()?;
    println!("{:?}", args);

    Ok(())
}
