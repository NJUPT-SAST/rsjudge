use std::{os::unix::process::CommandExt as _, process::Command};

use nix::unistd::setgroups;
use uzers::User;

pub mod user;
pub trait RunAs {
    fn run_as(&mut self, user: &User) -> &mut Command;
}

impl RunAs for Command {
    fn run_as(&mut self, user: &User) -> &mut Command {
        let uid = user.uid();
        let gid = user.primary_group_id();

        self.uid(uid).gid(gid);
        unsafe {
            self.pre_exec(move || {
                setgroups(&[gid.into()])?;
                Ok(())
            })
        };
        self
    }
}
