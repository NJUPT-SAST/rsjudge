use clap::Parser;

use crate::cli::Args;

mod cli;

fn main() {
    let args = Args::parse();
    println!("{:?}", args);
}
