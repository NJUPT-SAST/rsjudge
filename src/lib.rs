// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(not(test), warn(clippy::print_stdout, clippy::print_stderr))]
#![doc(html_logo_url = "https://cdn.jsdelivr.net/gh/NJUPT-SAST/rsjudge@main/assets/rsjudge.svg")]

//！ An online judge sandbox server in Rust,
//！ inspired by [go-judge](https://github.com/criyle/go-judge), for SASTOJ.

use anyhow::Context;
use log::{debug, warn};
use sysinfo::System;
use tokio::fs::read;

pub use crate::cli::Args;

mod cli;
mod config;

/// Main Entry point. This function assumes the global logger is correctly
/// setup.
///
/// # Errors
///
/// This function returns error if an internal error is not handled.
pub async fn async_main(args: Args) -> anyhow::Result<()> {
    debug!("{:?}", args);

    let executor_config_path = &args.config_dir.join("executors.toml");
    let config = read(executor_config_path).await.with_context(|| {
        format!(
            "Cannot load executor config at {}",
            executor_config_path.display()
        )
    })?;

    debug!(
        "Config:\n{:#?}",
        String::from_utf8_lossy(&config)
            .parse::<toml::Value>()
            .with_context(|| format!(
                "Failed to parse executor config at {}",
                executor_config_path.display()
            ))?
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
