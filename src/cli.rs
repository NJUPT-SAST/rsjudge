use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(author, about, version)]
pub(crate) struct Args {
    #[arg(short, long, default_value = "./config", value_name = "DIR")]
    /// Specify the configuration directory
    pub(crate) config_dir: PathBuf,
}
