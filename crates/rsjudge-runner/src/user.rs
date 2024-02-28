use anyhow::anyhow;
use once_cell::sync::Lazy;
use uzers::{get_user_by_name, User};

pub static SUPERVISOR: Lazy<Option<User>> = Lazy::new(|| get_user_by_name("rsjudge-supervisor"));
pub static BUILDER: Lazy<Option<User>> = Lazy::new(|| get_user_by_name("rsjudge-builder"));
pub static RUNNER: Lazy<Option<User>> = Lazy::new(|| get_user_by_name("rsjudge-runner"));

pub fn supervisor<'a>() -> anyhow::Result<&'a User> {
    SUPERVISOR
        .as_ref()
        .ok_or_else(|| anyhow!("User `rsjudge-supervisor` not found"))
}

pub fn builder<'a>() -> anyhow::Result<&'a User> {
    BUILDER
        .as_ref()
        .ok_or_else(|| anyhow!("User `rsjudge-builder` not found"))
}

pub fn runner<'a>() -> anyhow::Result<&'a User> {
    RUNNER
        .as_ref()
        .ok_or_else(|| anyhow!("User `rsjudge-runner` not found"))
}
