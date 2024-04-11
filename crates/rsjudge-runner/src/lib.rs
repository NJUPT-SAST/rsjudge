// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(not(test), warn(clippy::print_stdout, clippy::print_stderr))]

use std::{os::unix::process::CommandExt as _, process::Command};

use caps::Capability;
use nix::unistd::{setgroups, Gid};
use rsjudge_utils::log_if_error;
use uzers::User;

pub use crate::{
    caps_check::require_caps,
    error::{Error, Result},
};

mod caps_check;
mod error;

#[macro_use]
mod user_macro;
pub mod user;

pub trait RunAs {
    type Error;
    fn run_as(&mut self, user: &User) -> Result<&mut Self>;
}

impl RunAs for Command {
    type Error = Error;
    fn run_as(&mut self, user: &User) -> Result<&mut Self> {
        log_if_error!(require_caps([
            Capability::CAP_SETUID,
            Capability::CAP_SETGID,
            Capability::CAP_DAC_READ_SEARCH,
        ]))?;

        let uid = user.uid();
        let gid = user.primary_group_id();

        self.uid(uid).gid(gid);

        // SAFETY: `group` is moved into the closure,
        // and no longer accessible outside it.
        //
        // Replace with `CommandExt::groups` once it's stable.
        #[cfg(not(setgroups))]
        {
            let groups: Vec<_> = user
                .groups()
                .unwrap_or_default()
                .into_iter()
                .map(|g| Gid::from_raw(g.gid()))
                .collect();
            let set_groups = move || {
                log_if_error!(setgroups(&groups))?;
                Ok(())
            };
            unsafe { self.pre_exec(set_groups) };
        }

        #[cfg(setgroups)]
        {
            let groups: Vec<_> = user
                .groups()
                .unwrap_or_default()
                .into_iter()
                .map(|g| g.gid())
                .collect();

            self.groups(groups);
        }

        Ok(self)
    }
}
