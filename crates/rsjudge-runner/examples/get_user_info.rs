// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use capctl::Cap;
use rsjudge_runner::user::{builder, runner};
use rsjudge_runner::{RunAs, use_caps};
use tokio::process::Command;
use uzers::{get_current_uid, get_user_by_uid};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use_caps!(Cap::SETUID, Cap::SETGID);

    let self_output = Command::new("id")
        .run_as(&get_user_by_uid(get_current_uid()).ok_or_else(|| anyhow!("invalid user"))?)?
        .output()
        .await?;
    println!("{}", String::from_utf8_lossy(&self_output.stdout));

    let builder_output = Command::new("id").run_as(builder()?)?.output().await?;
    println!("{}", String::from_utf8_lossy(&builder_output.stdout));

    let runner_output = Command::new("id").run_as(runner()?)?.output().await?;
    println!("{}", String::from_utf8_lossy(&runner_output.stdout));

    Ok(())
}
