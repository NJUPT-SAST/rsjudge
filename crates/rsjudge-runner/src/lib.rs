use std::{os::unix::process::CommandExt as _, process::Command};

use nix::unistd::{setgroups, Gid};
use uzers::User;

pub mod user;
pub trait RunAs {
    fn run_as(&mut self, user: &User) -> &mut Command;
}

impl RunAs for Command {
    fn run_as(&mut self, user: &User) -> &mut Self {
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
            unsafe {
                self.pre_exec(move || {
                    setgroups(&groups)?;
                    Ok(())
                })
            };
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

        self
    }
}
