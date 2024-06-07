use std::{path::PathBuf, time::Duration};

use rsjudge_runner::utils::resources::{rusage::WaitForResourceUsage, RunWithResourceLimit};
use rsjudge_traits::resource::ResourceLimit;
use tokio::{process::Command, time::Instant};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .ok_or_else(|| anyhow::anyhow!("cannot find crate root"))?
        .join("target/debug/examples");
    let spin_lock = examples.join("spin_lock");
    eprintln!("Starting spin_lock with CPU time limit of 1s, wall time limit 2s:");
    let start_time = Instant::now();
    let _ = Command::new(spin_lock)
        .spawn_with_resource_limit(ResourceLimit::new(
            Some(Duration::from_secs(1)),
            Some(Duration::from_secs(2)),
            None,
            None,
        ))?
        .wait_for_resource_usage()
        .await;

    dbg!(start_time.elapsed());
    let sleep = examples.join("sleep");
    eprintln!("Starting sleep with CPU time limit of 1s, wall time limit 2s:");
    let start_time = Instant::now();
    let _ = Command::new(sleep)
        .spawn_with_resource_limit(ResourceLimit::new(
            Some(Duration::from_secs(1)),
            Some(Duration::from_secs(2)),
            None,
            None,
        ))?
        .wait_for_resource_usage()
        .await;

    dbg!(start_time.elapsed());
    Ok(())
}
