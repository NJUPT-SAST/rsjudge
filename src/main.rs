// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(not(test), warn(clippy::print_stdout, clippy::print_stderr))]

use log::error;
#[cfg(feature = "mimalloc")]
use mimalloc::MiMalloc;
use rsjudge::async_main;

use crate::logging::setup_logger;

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

mod cli;
mod logging;

#[tokio::main]
async fn main() {
    setup_logger();

    if let Err(err) = async_main().await {
        error!("{:?}", err);
    }
}
