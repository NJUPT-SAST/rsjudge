use std::process::Command;

use rsjudge_runner::{
    user::{builder, runner},
    RunAs,
};
fn main() -> anyhow::Result<()> {
    let builder_output = Command::new("id").run_as(builder()?).output()?;
    println!("{}", String::from_utf8_lossy(&builder_output.stdout));

    let runner_output = Command::new("id").run_as(runner()?).output()?;
    println!("{}", String::from_utf8_lossy(&runner_output.stdout));
    Ok(())
}
