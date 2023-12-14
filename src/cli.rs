use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(author, about, version)]
pub(crate) struct Args {
    #[arg(short, long, default_value = "config")]
    /// The path to the configuration files directory.
    pub(crate) config: PathBuf,
}
