use std::process::Command;

use anyhow::anyhow;
use rsjudge_runner::{
    user::{builder, runner},
    RunAs,
};
use uzers::{get_current_uid, get_user_by_uid};
fn main() -> anyhow::Result<()> {
    let self_output = Command::new("id")
        .run_as(&get_user_by_uid(get_current_uid()).ok_or(anyhow!("invalid user"))?)?
        .output()?;
    println!("{}", String::from_utf8_lossy(&self_output.stdout));

    let builder_output = Command::new("id").run_as(builder()?)?.output()?;
    println!("{}", String::from_utf8_lossy(&builder_output.stdout));

    let runner_output = Command::new("id").run_as(runner()?)?.output()?;
    println!("{}", String::from_utf8_lossy(&runner_output.stdout));
    Ok(())
}
