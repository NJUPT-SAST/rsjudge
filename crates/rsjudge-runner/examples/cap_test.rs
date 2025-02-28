// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use capctl::{Cap, CapState};
use rsjudge_runner::{RunAs, use_caps, user::builder};
use rsjudge_utils::command::check_output;
use tokio::process::Command;

/// An attempt to exploit the runner by running a binary with a setuid call.
///
/// To run this example, make sure to compile `exploit_inner` and `normal`,
/// and set the capabilities on the `exploit` binary.
///
/// You can use xtask to achieve this:
///
/// ```sh
/// cargo xtask cap-test
/// ```
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dbg!(CapState::get_current().unwrap());
    eprintln!("Safely rise the required capabilities.");
    use_caps!(Cap::SETUID, Cap::SETGID, Cap::DAC_READ_SEARCH);

    dbg!(CapState::get_current().unwrap());

    // Get the path to the examples.
    // This crate is located at crates/rsjudge-runner,
    // so we need to go up two levels to find the workspace root.
    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .ok_or_else(|| anyhow::anyhow!("cannot find crate root"))?
        .join("target/debug/examples");

    let exploit = examples.join("exploit");

    let normal = examples.join("normal");
    eprintln!("Starting normal program");
    let status = check_output(Command::new(normal).run_as(builder()?)?).await?;
    println!("{}", String::from_utf8_lossy(&status.stdout));
    eprintln!("{}", String::from_utf8_lossy(&status.stderr));

    eprintln!("Starting evil program.");
    let status = check_output(Command::new(exploit).run_as(builder()?)?).await?;
    println!("{}", String::from_utf8_lossy(&status.stdout));
    eprintln!("{}", String::from_utf8_lossy(&status.stderr));

    eprintln!("Starting captest program for full testing.");
    let status = check_output(Command::new("captest").run_as(builder()?)?).await?;
    println!("{}", String::from_utf8_lossy(&status.stdout));
    eprintln!("{}", String::from_utf8_lossy(&status.stderr));

    Ok(())
}
