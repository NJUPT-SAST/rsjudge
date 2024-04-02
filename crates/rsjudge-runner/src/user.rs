use std::sync::OnceLock;

use uzers::{get_user_by_name, User};

use crate::error::{Error, Result};

pub static SUPERVISOR: OnceLock<Option<User>> = OnceLock::new();
pub static BUILDER: OnceLock<Option<User>> = OnceLock::new();
pub static RUNNER: OnceLock<Option<User>> = OnceLock::new();

pub fn supervisor<'a>() -> Result<&'a User> {
    SUPERVISOR
        .get_or_init(|| get_user_by_name("rsjudge-supervisor"))
        .as_ref()
        .ok_or_else(|| Error::UserNotFound {
            name: "rsjudge-supervisor".to_string(),
        })
}

pub fn builder<'a>() -> Result<&'a User> {
    BUILDER
        .get_or_init(|| get_user_by_name("rsjudge-builder"))
        .as_ref()
        .ok_or_else(|| Error::UserNotFound {
            name: "rsjudge-builder".to_string(),
        })
}

pub fn runner<'a>() -> Result<&'a User> {
    RUNNER
        .get_or_init(|| get_user_by_name("rsjudge-runner"))
        .as_ref()
        .ok_or_else(|| Error::UserNotFound {
            name: "rsjudge-runner".to_string(),
        })
}
