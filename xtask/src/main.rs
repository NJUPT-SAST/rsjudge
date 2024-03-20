use std::{env::set_current_dir, path::Path};

use clap::{Parser, ValueEnum};
use xshell::{cmd, Shell};

#[derive(Debug, ValueEnum, Clone, Copy, PartialEq, Eq)]
/// Package distribution-specific packages.
enum Package {
    /// Build DEB package.
    Deb,
    /// Build RPM package (unimplemented).
    Rpm,
}

#[derive(Debug, Parser)]
#[clap(about, long_about)]
/// Build related tasks.
enum Command {
    /// Package distribution-specific packages.
    Dist {
        /// Which package to build.
        #[arg(value_enum)]
        package: Package,
    },
    /// Build Docker image.
    Docker,
    /// Debug a command.
    #[cfg(feature = "dbg")]
    Debug,
}

fn main() -> anyhow::Result<()> {
    let command = Command::parse();

    // chdir to the workspace root so that the xtask can be invoked from anywhere.
    // Assume that the xtask is in {project_root}/xtask
    set_current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap())?;

    let sh = Shell::new()?;
    {
        match command {
            Command::Dist { package } => match package {
                Package::Deb => cmd!(sh, "cargo deb -v"),
                Package::Rpm => todo!("Not implemented"),
            },
            Command::Docker => cmd!(sh, "docker build -t rsjudge ."),
            #[cfg(feature = "dbg")]
            Command::Debug => return Ok(()),
        }
        .run()?
    }
    Ok(())
}
