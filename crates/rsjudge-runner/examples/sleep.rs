use std::{path::PathBuf, time::Duration};

use capctl::{Cap, CapState};
use rsjudge_runner::{
    use_caps,
    utils::resources::{rusage::WaitForResourceUsage, RunWithResourceLimit},
};
use rsjudge_traits::resource::ResourceLimit;
use tokio::process::Command;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use_caps!(Cap::DAC_READ_SEARCH);
    dbg!(CapState::get_current().unwrap());

    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .ok_or_else(|| anyhow::anyhow!("cannot find crate root"))?
        .join("target/debug/examples");
    let sleep_inner = examples.join("sleep_inner");
    dbg!(&sleep_inner);
    let status = Command::new(sleep_inner)
        .spawn_with_resource_limit(ResourceLimit::new(
            Some(Duration::from_secs(1)),
            Some(Duration::from_secs(2)),
            None,
            None,
        ))?
        .wait_for_resource_usage()
        .await?;
    dbg!(status);
    Ok(())
}
