#[path = "src/cli.rs"]
mod cli;

use std::{
    env::var_os,
    fs,
    io::{self, ErrorKind},
    path::PathBuf,
};

use clap::CommandFactory;
use clap_mangen::Man;

use crate::cli::Args;

fn main() -> io::Result<()> {
    shadow_rs::new().unwrap();
    let out_dir = PathBuf::from(var_os("OUT_DIR").ok_or(ErrorKind::NotFound)?);

    let mut buffer: Vec<u8> = Vec::new();

    Man::new(Args::command()).render(&mut buffer)?;

    fs::write(out_dir.join("rsjudge.1"), buffer)?;

    Ok(())
}
