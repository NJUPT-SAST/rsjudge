// SPDX-License-Identifier: Apache-2.0

use std::{env::set_current_dir, io, path::Path, process::Command};

use clap::{Parser, ValueEnum};

#[derive(Debug, ValueEnum, Clone, Copy, PartialEq, Eq)]
/// Package distribution-specific packages.
enum Package {
    /// Build DEB package.
    Deb,
    /// Build RPM package.
    Rpm,
}

#[derive(Debug, Parser)]
#[clap(about, long_about)]
/// Build related tasks.
enum Args {
    /// Package distribution-specific packages.
    Dist {
        /// Which package to build.
        #[arg(value_enum)]
        pkg: Package,
    },
    /// Build Docker image.
    Docker,
    /// Debug a command.
    #[cfg(feature = "dbg")]
    Debug,
}

fn exec(program: &str, args: &[&str]) -> io::Result<()> {
    Command::new(program).args(args).spawn()?.wait()?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // chdir to the workspace root so that `cargo xtask` can be invoked from anywhere.
    set_current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap())?;

    match args {
        Args::Dist { pkg: Package::Deb } => exec("cargo", &["deb", "-v", "--locked"])?,
        Args::Dist { pkg: Package::Rpm } => {
            // `cargo-generate-rpm` does not invoke `cargo build` itself.
            exec("cargo", &["build", "--release", "--locked"])?;
            exec("cargo", &["generate-rpm"])?;
        }
        Args::Docker => exec("docker", &["build", "-t", "rsjudge", "."])?,
        #[cfg(feature = "dbg")]
        Args::Debug => {}
    }

    Ok(())
}
