// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(not(test), warn(clippy::print_stdout, clippy::print_stderr))]

use clap::Parser as _;
use log::error;
#[cfg(feature = "mimalloc")]
use mimalloc::MiMalloc;
use rsjudge::{Args, async_main};

use crate::logging::setup_logger;

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

mod cli;
mod logging;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    setup_logger();

    if let Err(err) = async_main(args).await {
        error!("{err:#}");
        Err(err)?
    } else {
        Ok(())
    }
}
