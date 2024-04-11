// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(not(test), warn(clippy::print_stdout, clippy::print_stderr))]
#![doc(html_logo_url = "https://cdn.jsdelivr.net/gh/NJUPT-SAST/rsjudge@main/assets/rsjudge.svg")]

//！ An online judge sandbox server in Rust,
//！ inspired by [go-judge](https://github.com/criyle/go-judge), for SAST OJ.

use std::process::Command;

use clap::Parser as _;
use env_logger::Env;
use log::{debug, info, warn};
use rsjudge_runner::{user::builder, RunAs as _};
use rsjudge_utils::command::display_cmd;
use tokio::fs::read;

use crate::cli::Args;

mod cli;

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

    info!("Executing \"id\" as rsjudge-builder");

    match Command::new("id").run_as(builder()?) {
        Ok(it) => {
            debug!("{} exited with {}", display_cmd(it), it.status()?);
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
