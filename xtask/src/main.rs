// SPDX-License-Identifier: Apache-2.0

use std::{env::set_current_dir, path::Path};

use clap::{Parser, ValueEnum};
use sh::cmd;

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
enum Command {
    /// Package distribution-specific packages.
    Dist {
        /// Which package to build.
        #[arg(value_enum)]
        package: Package,
    },
    /// Build Docker image.
    Docker,
    /// Run integrated capability checks on project.
    ///
    /// Requires `sudo` to set capabilities.
    CapTest,
    /// Run integrated sleep test.
    RusageTest,
    /// Debug a command.
    #[cfg(feature = "dbg")]
    Debug,
}

fn main() -> anyhow::Result<()> {
    let command = Command::parse();

    // chdir to the workspace root so that `cargo xtask` can be invoked from anywhere.
    set_current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap())?;

    match command {
        Command::Dist { package } => match package {
            Package::Deb => cmd!(cargo deb "-v"),
            Package::Rpm => cmd! {
                cargo build "--release";
                cargo "generate-rpm"
            },
        },
        Command::Docker => cmd!(docker build "-t" rsjudge "."),
        Command::CapTest => cmd! {
            cargo build "--examples" "--workspace";

            echo "Setting capabilities for get_user_info and cap_test binaries with sudo.";

            sudo setcap "cap_setuid,cap_setgid=p" "target/debug/examples/get_user_info";
            sudo setcap "cap_setuid,cap_setgid,cap_dac_read_search=p" "target/debug/examples/cap_test";

            "target/debug/examples/get_user_info";
            "target/debug/examples/cap_test";
        },
        Command::RusageTest => cmd! {
            cargo build "--examples" "--workspace";

            "target/debug/examples/rusage_test";
        },

        #[cfg(feature = "dbg")]
        Command::Debug => return Ok(()),
    }
    .try_for_each(|cmd| {
        #[cfg(feature = "dbg")]
        eprintln!("Executing: {:?}", cmd);
        cmd.exec()
    })?;

    Ok(())
}
