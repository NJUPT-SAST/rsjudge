// SPDX-License-Identifier: Apache-2.0

use std::io::{self, ErrorKind};

use capctl::Cap;
use nix::unistd::{setgroups, Gid};
use rsjudge_utils::log_if_error;
use tokio::process::Command;
use uzers::User;

use crate::{
    cap_handle::CapHandle,
    error::{Error, Result},
};

/// A trait to allow running a [`tokio::process::Command`] as another user.
pub trait RunAs {
    type Error;

    /// Run the [`Command`] as the given [`User`].
    ///
    /// This function will set the UID, GID, and supplementary groups of the command.
    ///
    /// # Errors
    ///
    /// This function will return an error if the user does not exist,
    /// or if the process does not have the necessary capabilities.
    fn run_as(&mut self, user: &User) -> Result<&mut Self>;
}

impl RunAs for Command {
    type Error = Error;

    fn run_as(&mut self, user: &User) -> Result<&mut Self> {
        let uid = user.uid();
        let gid = user.primary_group_id();

        self.uid(uid).gid(gid);

        #[cfg(not(setgroups))]
        {
            let groups: Vec<_> = user
                .groups()
                .unwrap_or_default()
                .into_iter()
                .map(|g| Gid::from_raw(g.gid()))
                .collect();

            let set_groups = move || {
                CapHandle::new(Cap::SETGID)
                    .map_err(|e| io::Error::new(ErrorKind::PermissionDenied, e.to_string()))?;
                log_if_error!(setgroups(&groups))?;
                Ok(())
            };

            // SAFETY: `groups` is moved into the `set_groups` closure,
            // and no longer accessible outside it.
            //
            // Replace with `CommandExt::groups` once it's stable.
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
