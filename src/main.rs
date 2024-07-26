// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(not(test), warn(clippy::print_stdout, clippy::print_stderr))]

use log::error;
#[cfg(feature = "mimalloc")]
use mimalloc::MiMalloc;
use rsjudge::async_main;

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

mod cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if let Err(err) = async_main().await {
        error!("{:?}", err);
    }
    Ok(())
}
