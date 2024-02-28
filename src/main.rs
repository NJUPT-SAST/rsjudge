use std::process::Command;

use caps::{has_cap, CapSet, Capability};
use clap::Parser;
use env_logger::Env;
use log::{debug, info, trace};
use rsjudge_runner::{user::builder, RunAs};
use tokio::fs::read;

use crate::cli::Args;
mod cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(
        Env::new()
            .filter_or("RSJUDGE_LOG", "info")
            .write_style("RSJUDGE_LOG_STYLE"),
    )
    .format_timestamp_millis()
    .format_module_path(true)
    .try_init()?;

    let args = Args::try_parse()?;
    info!("{:?}", args);

    let config = read(args.config_dir.join("executors.toml")).await?;
    if has_cap(None, CapSet::Permitted, Capability::CAP_SETUID)?
        && has_cap(None, CapSet::Permitted, Capability::CAP_SETGID)?
    {
        debug!("Executing `id` as `rsjudge-builder`");
        info!("{}", Command::new("id").run_as(builder()?).status()?);
    } else {
        info!("CAP_SETUID and CAP_SETGID not set, skipping.");
    }

    trace!(
        "Config:\n{:#?}",
        String::from_utf8_lossy(&config).parse::<toml::Value>()?
    );

    Ok(())
}
