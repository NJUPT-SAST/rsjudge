use std::env::set_current_dir;

use anyhow::anyhow;
use clap::{Parser, ValueEnum};
use dist::{build_script_out_dir, deb::deb_package, Profile};
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
    /// Generate Rust modules from Protobuf definitions.
    Codegen,
    /// Package distribution-specific packages.
    Dist {
        /// Which package to build.
        #[arg(value_enum)]
        package: Package,
    },
    /// Build Docker image.
    Docker,
    /// Debug a command.
    Debug,
}

mod dist;

fn main() -> anyhow::Result<()> {
    let command = Command::parse();

    // chdir to the workspace root so that the xtask can be invoked from anywhere.
    // Assume that the xtask is in {project_root}/xtask
    set_current_dir(format!("{}/..", env!("CARGO_MANIFEST_DIR")))?;

    let sh = Shell::new()?;
    {
        match command {
            Command::Codegen => cmd!(sh, "echo Not implemented"),
            Command::Dist { package } => match package {
                Package::Deb => return Ok(deb_package(sh)?),
                Package::Rpm => Err(anyhow!("Not implemented"))?,
            },
            Command::Docker => cmd!(sh, "docker build -t rsjudge ."),
            Command::Debug => {
                return Ok(println!(
                    "{:#?}",
                    build_script_out_dir(&sh, Profile::Debug)?
                ))
            }
        }
        .run()?
    }
    Ok(())
}
