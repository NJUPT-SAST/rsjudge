use std::sync::OnceLock;

use anyhow::anyhow;
use uzers::{get_user_by_name, User};

static SUPERVISOR_LOCK: OnceLock<Option<User>> = OnceLock::new();
static BUILDER_LOCK: OnceLock<Option<User>> = OnceLock::new();
static RUNNER_LOCK: OnceLock<Option<User>> = OnceLock::new();

pub fn supervisor() -> anyhow::Result<&'static User> {
    SUPERVISOR_LOCK
        .get_or_init(|| get_user_by_name("rsjudge-supervisor"))
        .as_ref()
        .ok_or_else(|| anyhow!("No such user: rsjudge-supervisor"))
}

pub fn builder() -> anyhow::Result<&'static User> {
    BUILDER_LOCK
        .get_or_init(|| get_user_by_name("rsjudge-builder"))
        .as_ref()
        .ok_or_else(|| anyhow!("No such user: rsjudge-builder"))
}

pub fn runner() -> anyhow::Result<&'static User> {
    RUNNER_LOCK
        .get_or_init(|| get_user_by_name("rsjudge-runner"))
        .as_ref()
        .ok_or_else(|| anyhow!("No such user: rsjudge-runner"))
}

#[cfg(all(test, unix))]
mod tests {
    use std::{os::unix::process::CommandExt, process::Command};

    use super::*;

    #[test]
    #[ignore = "Requires additional users."]
    fn test_uid() -> anyhow::Result<()> {
        let builder = builder()?;
        let builder_output = Command::new("id")
            .uid(builder.uid())
            .gid(builder.primary_group_id())
            .output()?;
        println!("{}", String::from_utf8_lossy(&builder_output.stdout));
        let runner = runner()?;
        let runner_output = Command::new("id")
            .uid(runner.uid())
            .gid(runner.primary_group_id())
            .output()?;
        println!("{}", String::from_utf8_lossy(&runner_output.stdout));
        Ok(())
    }
}
