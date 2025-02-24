// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(not(test), warn(clippy::print_stdout, clippy::print_stderr))]
#![doc(html_logo_url = "https://cdn.jsdelivr.net/gh/NJUPT-SAST/rsjudge@main/assets/rsjudge.svg")]

//！ An online judge sandbox server in Rust,
//！ inspired by [go-judge](https://github.com/criyle/go-judge), for SASTOJ.

use clap::Parser as _;
use log::debug;
use sysinfo::System;
use tokio::fs::read;

use crate::cli::Args;

mod cli;
mod config;

/// Main Entry point. This function assumes the global logger is correctly setup.
///
/// # Errors
///
/// This function returns error if an internal error is not handled.
pub async fn async_main() -> anyhow::Result<()> {
    let args = Args::try_parse()?;
    debug!("{:?}", args);

    let config = read(args.config_dir.join("executors.toml")).await?;

    debug!(
        "Config:\n{:#?}",
        String::from_utf8_lossy(&config).parse::<toml::Value>()?
    );

    debug!(
        "System Version: {}",
        System::long_os_version()
            .as_deref()
            .unwrap_or("Unspecified")
    );

    Ok(())
}
