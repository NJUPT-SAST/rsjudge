// SPDX-License-Identifier: Apache-2.0

use capctl::Cap;
#[cfg(not(setgroups))]
use nix::unistd::{setgroups, Gid};
use rsjudge_utils::log_if_error;
use tokio::process::Command;
use uzers::User;

use crate::{
    error::{Error, Result},
    utils::cap_handle::CapHandle,
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

        let groups: Vec<_> = user
            .groups()
            .unwrap_or_default()
            .into_iter()
            .map(|g| g.gid())
            .collect();

        self.set_groups(&groups);

        Ok(self)
    }
}

trait SetGroups {
    fn set_groups(&mut self, groups: &[u32]) -> &mut Self;
}

impl SetGroups for Command {
    #[cfg(not(setgroups))]
    fn set_groups(&mut self, groups: &[u32]) -> &mut Self {
        use std::io::{self, ErrorKind};

        let groups: Vec<_> = groups.iter().map(|&g| Gid::from_raw(g)).collect();

        let set_groups = move || {
            CapHandle::new(Cap::SETGID)
                .map_err(|e| io::Error::new(ErrorKind::PermissionDenied, e.to_string()))?;
            log_if_error!(setgroups(&groups))?;
            Ok(())
        };

        unsafe { self.pre_exec(set_groups) };

        self
    }

    #[cfg(setgroups)]
    fn set_groups(&mut self, groups: &[u32]) -> &mut Self {
        use std::os::unix::process::CommandExt as _;

        let _set_groups_handle = log_if_error!(CapHandle::new(Cap::SETGID));

        self.as_std_mut().groups(groups);
        self
    }
}
