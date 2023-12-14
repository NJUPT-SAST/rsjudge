use clap::Parser;
use shadow_rs::shadow;

use crate::cli::Args;

mod cli;

shadow!(shadow_build);

fn main() {
    let args = Args::parse();
    println!("{:?}", args);
}
