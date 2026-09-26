// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(not(test), warn(clippy::print_stdout, clippy::print_stderr))]
#![doc(html_logo_url = "https://cdn.jsdelivr.net/gh/NJUPT-SAST/rsjudge@main/assets/rsjudge.svg")]

//！ An online judge sandbox server in Rust,
//！ inspired by [go-judge](https://github.com/criyle/go-judge), for SASTOJ.

use log::{debug, warn};
use sysinfo::System;

pub use crate::cli::Args;
pub use crate::config::Config;

mod cli;
mod config;

/// Main Entry point. This function assumes the global logger is correctly
/// setup.
///
/// # Errors
///
/// This function returns error if an internal error is not handled.
pub async fn async_main(args: Args) -> anyhow::Result<()> {
    debug!("{args:?}");

    let config = Config::load_from_dir(&args.config_dir).await?;

    debug!("Loaded {} executors", config.executors.len());
    debug!(
        "Seccomp default action: {:?}",
        config.seccomp.default_action
    );

    match (System::name(), System::os_version()) {
        (Some(distro_name), Some(distro_version)) => {
            debug!("OS: {distro_name} {distro_version}");
        }
        (Some(distro_name), None) => {
            debug!("OS: {distro_name}");
        }
        _ => {
            warn!("Failed to detect OS information.")
        }
    }

    Ok(())
}
