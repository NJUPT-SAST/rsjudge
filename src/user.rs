use std::sync::OnceLock;

use uzers::{get_group_by_name, get_user_by_name};

static BUILDER_UID_LOCK: OnceLock<Option<u32>> = OnceLock::new();
static BUILDER_GID_LOCK: OnceLock<Option<u32>> = OnceLock::new();
static RUNNER_UID_LOCK: OnceLock<Option<u32>> = OnceLock::new();
static RUNNER_GID_LOCK: OnceLock<Option<u32>> = OnceLock::new();

pub fn builder_uid() -> Option<u32> {
    *BUILDER_UID_LOCK.get_or_init(|| get_user_by_name("rsjudge-builder").map(|u| u.uid()))
}

pub fn builder_gid() -> Option<u32> {
    *BUILDER_GID_LOCK.get_or_init(|| get_group_by_name("rsjudge-builder").map(|g| g.gid()))
}

pub fn runner_uid() -> Option<u32> {
    *RUNNER_UID_LOCK.get_or_init(|| get_user_by_name("rsjudge-runner").map(|u| u.uid()))
}

pub fn runner_gid() -> Option<u32> {
    *RUNNER_GID_LOCK.get_or_init(|| get_group_by_name("rsjudge-runner").map(|g| g.gid()))
}

#[cfg(all(test, unix))]
mod tests {
    use std::{os::unix::process::CommandExt, process::Command};

    use anyhow::anyhow;

    use super::*;
    #[test]
    #[ignore = "Requires additional users."]
    fn test_uid() -> anyhow::Result<()> {
        let builder_output = Command::new("id")
            .uid(builder_uid().ok_or_else(|| anyhow!("No such user: rsjudge-builder"))?)
            .gid(builder_gid().ok_or_else(|| anyhow!("No such group: rsjudge-builder"))?)
            .output()?;
        println!("{}", String::from_utf8_lossy(&builder_output.stdout));
        let runner_output = Command::new("id")
            .uid(runner_uid().ok_or_else(|| anyhow!("No such user: rsjudge-runner"))?)
            .gid(runner_gid().ok_or_else(|| anyhow!("No such group: rsjudge-runner"))?)
            .output()?;
        println!("{}", String::from_utf8_lossy(&runner_output.stdout));
        Ok(())
    }
}
