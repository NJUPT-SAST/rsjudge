// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(author, about, version)]
pub struct Args {
    #[arg(short, long, default_value = "./config", value_name = "DIR")]
    /// Specify the configuration directory
    pub config_dir: PathBuf,
}
