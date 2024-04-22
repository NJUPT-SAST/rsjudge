// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(not(test), warn(clippy::print_stdout, clippy::print_stderr))]
#![doc(html_logo_url = "https://cdn.jsdelivr.net/gh/NJUPT-SAST/rsjudge@main/assets/rsjudge.svg")]

//！ An online judge sandbox server in Rust,
//！ inspired by [go-judge](https://github.com/criyle/go-judge), for SASTOJ.

use clap::Parser as _;
use env_logger::Env;
use log::{debug, info, warn};
use rsjudge_runner::{user::builder, Cap, CapHandle, RunAs as _};
use tokio::{fs::read, process::Command};

use crate::cli::Args;

mod cli;
mod config;

/// # Errors
///
pub async fn main_impl() -> anyhow::Result<()> {
    env_logger::Builder::from_env(
        Env::new()
            .filter_or("RSJUDGE_LOG", "info")
            .write_style("RSJUDGE_LOG_STYLE"),
    )
    .format_timestamp_millis()
    .format_module_path(true)
    .try_init()?;

    let args = Args::try_parse()?;
    debug!("{:?}", args);

    let config = read(args.config_dir.join("executors.toml")).await?;

    info!("Executing \"captest\" as rsjudge-builder");

    CapHandle::new(Cap::SETUID)?;
    CapHandle::new(Cap::SETGID)?;

    match Command::new("captest")
        .run_as(builder()?)
        .and_then(|cmd| Ok(cmd.spawn()?))
    {
        Ok(mut child) => {
            debug!("Command exited with {}", child.wait().await?);
        }
        Err(err) => {
            warn!("Failed to run \"id\" as rsjudge-builder: {}", err);
        }
    };

    debug!(
        "Config:\n{:#?}",
        String::from_utf8_lossy(&config).parse::<toml::Value>()?
    );

    Ok(())
}
