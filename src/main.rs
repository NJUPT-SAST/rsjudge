// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(not(test), warn(clippy::print_stdout, clippy::print_stderr))]

use log::error;
use mimalloc::MiMalloc;
use rsjudge::main_impl;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

mod cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if let Err(err) = main_impl().await {
        error!("{:?}", err);
    }
    Ok(())
}
