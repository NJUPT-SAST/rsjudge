// SPDX-License-Identifier: Apache-2.0

use std::{num::NonZeroU64, os::unix::process::ExitStatusExt, path::PathBuf, time::Duration};

use anyhow::bail;
use nix::{sys::wait::WaitStatus, unistd::Pid};
use rsjudge_runner::utils::resources::{WithResourceLimit as _, rusage::WaitForResourceUsage};
use rsjudge_traits::resource::ResourceLimit;
use tokio::{process::Command, time::Instant};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .ok_or_else(|| anyhow::anyhow!("cannot find crate root"))?
        .join("target/debug/examples");
    let spin_lock = examples.join("spinning");
    eprintln!("Starting spin_lock with CPU time limit of 1s, wall time limit 2s:");
    let start_time = Instant::now();
    let (status, rusage) = Command::new(spin_lock)
        .spawn_with_resource_limit(ResourceLimit::new(
            Some(Duration::from_secs(1)),
            Some(Duration::from_secs(2)),
            None,
            None,
        ))?
        .wait_for_resource_usage()
        .await?;

    dbg!(start_time.elapsed());
    let status = WaitStatus::from_raw(Pid::from_raw(0), status.into_raw())?;
    dbg!(status);
    dbg!(rusage.cpu_time());

    let sleep = examples.join("sleep");
    eprintln!("Starting sleep with CPU time limit of 1s, wall time limit 2s:");
    let start_time = Instant::now();
    let Err(e) = Command::new(sleep)
        .spawn_with_resource_limit(ResourceLimit::new(
            Some(Duration::from_secs(1)),
            Some(Duration::from_secs(2)),
            None,
            None,
        ))?
        .wait_for_resource_usage()
        .await
    else {
        bail!("Failed to get resource usage for `{}`", stringify!(sleep));
    };

    dbg!(start_time.elapsed());
    dbg!(e);

    let large_alloc = examples.join("large_alloc");
    eprintln!("Starting `large_alloc` with RAM limit of 1MB");

    let Ok((status, rusage)) = Command::new(large_alloc)
        .spawn_with_resource_limit(ResourceLimit::new(
            None,
            None,
            NonZeroU64::new(1 << 30),
            None,
        ))?
        .wait_for_resource_usage()
        .await
    else {
        bail!(
            "Failed to get resource usage for `{}`",
            stringify!(large_alloc)
        );
    };

    let status = WaitStatus::from_raw(Pid::from_raw(0), status.into_raw())?;
    dbg!(status);
    dbg!(rusage);

    Ok(())
}
