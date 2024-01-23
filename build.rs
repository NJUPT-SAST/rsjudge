#[path = "src/cli.rs"]
mod cli;

use std::{
    env::var_os,
    fs::File,
    io::{self, ErrorKind, Write},
    path::PathBuf,
};

use clap::CommandFactory;
use clap_complete::{generate_to, shells::Shell};
use clap_mangen::Man;

use crate::cli::Args;

fn main() -> anyhow::Result<()> {
    let out_dir = PathBuf::from(var_os("OUT_DIR").ok_or(io::Error::from(ErrorKind::NotFound))?);

    let mut cmd = Args::command();

    for shell in [Shell::Bash, Shell::Fish, Shell::Zsh] {
        generate_to(shell, &mut cmd, "rsjudge", &out_dir)?;
    }

    let mut manpage = File::create(out_dir.join("rsjudge.1"))?;
    Man::new(cmd).render(&mut manpage)?;
    manpage.flush()?;

    println!("cargo:rerun-if-changed=src/cli.rs");

    Ok(())
}
