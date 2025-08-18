// SPDX-License-Identifier: Apache-2.0

#[path = "src/cli.rs"]
mod cli;

use std::env::var_os;
use std::fs::{File, create_dir_all};
use std::io::{self, Write};
use std::path::Path;

use clap::CommandFactory;
use clap_complete::generate_to;
use clap_complete::shells::Shell;
use clap_mangen::Man;

use crate::cli::Args;

fn main() -> io::Result<()> {
    let asset_dir = Path::new(
        &var_os("CARGO_MANIFEST_DIR").expect("Environment `CARGO_MANIFEST_DIR` not set by cargo."),
    )
    .join("target/assets");

    create_dir_all(&asset_dir)?;

    let mut cmd = Args::command();

    for shell in [Shell::Bash, Shell::Fish, Shell::Zsh] {
        generate_to(shell, &mut cmd, "rsjudge", &asset_dir)?;
    }

    let mut manpage = File::create(asset_dir.join("rsjudge.1"))?;
    Man::new(cmd).render(&mut manpage)?;
    manpage.flush()?;

    println!("cargo:rerun-if-changed=src/cli.rs");

    Ok(())
}
