// SPDX-License-Identifier: Apache-2.0

use capctl::Cap;
#[cfg(not(feature = "setgroups"))]
use nix::unistd::{Gid, setgroups};
use rsjudge_utils::log_if_error;
use tokio::process::Command;
use uzers::User;

use crate::config::SeccompFilter;
use crate::error::{Error, Result};
use crate::utils::cap_handle::CapHandle;

/// Drop **all** capabilities from the child's bounding set.
///
/// The bounding set is a per-thread ceiling on the capabilities a process
/// can ever obtain. Clearing it guarantees that the user program stays
/// without any capability, and `PR_SET_NO_NEW_PRIVS` additionally covers
/// the setuid-binary path.
///
/// Dropping from the bounding set requires `CAP_SETPCAP`, which is raised
/// temporarily for the duration of this hook via [`CapHandle`] and reclaimed
/// afterwards.
///
/// **Ordering**: register this *after* [`RunAs::run_as`] (which still needs
/// `CAP_SETGID`) and *before* [`WithSeccomp::with_seccomp`].
pub trait DropBoundingCaps {
    /// Register a `pre_exec` hook that clears the bounding set.
    fn drop_bounding_caps(&mut self) -> Result<&mut Self>;
}

impl DropBoundingCaps for Command {
    fn drop_bounding_caps(&mut self) -> Result<&mut Self> {
        let drop = move || -> std::io::Result<()> {
            // Raise CAP_SETPCAP for the duration of the bounding-set cleanup.
            let _handle = CapHandle::new(Cap::SETPCAP)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::PermissionDenied, e))?;

            // Drop every capability capctl knows about.
            for cap in Cap::iter() {
                if capctl::caps::bounding::read(cap) == Some(true) {
                    capctl::caps::bounding::drop(cap).map_err(std::io::Error::other)?;
                }
            }
            // Drop any capabilities newer than this version of capctl.
            capctl::caps::bounding::clear_unknown().map_err(std::io::Error::other)?;

            Ok(())
        };

        // SAFETY: the hook only calls `prctl` (capability manipulation),
        // which is async-signal-safe.
        unsafe { self.pre_exec(drop) };

        Ok(self)
    }
}

/// A trait to allow running a [`tokio::process::Command`] as another user.
pub trait RunAs {
    type Error;

    /// Run the [`Command`] as the given [`User`].
    ///
    /// This function will set the UID, GID, and supplementary groups of the
    /// command.
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
    #[cfg(not(feature = "setgroups"))]
    fn set_groups(&mut self, groups: &[u32]) -> &mut Self {
        let groups: Vec<_> = groups.iter().map(|&g| Gid::from_raw(g)).collect();

        let set_groups = move || {
            use std::io::{self, ErrorKind};

            CapHandle::new(Cap::SETGID)
                .map_err(|e| io::Error::new(ErrorKind::PermissionDenied, e))?;
            log_if_error!(setgroups(&groups))?;
            Ok(())
        };

        unsafe { self.pre_exec(set_groups) };

        self
    }

    #[cfg(feature = "setgroups")]
    fn set_groups(&mut self, groups: &[u32]) -> &mut Self {
        use std::os::unix::process::CommandExt as _;

        let _set_groups_handle = log_if_error!(CapHandle::new(Cap::SETGID));

        self.as_std_mut().groups(groups);
        self
    }
}

/// A trait to apply a compiled seccomp filter to a [`Command`].
///
/// # Required call order
///
/// `pre_exec` hooks run in registration order inside the forked child, and
/// once a seccomp filter is loaded it constrains **every** subsequent
/// syscall — including the remaining setup hooks and `execve` itself.
/// Therefore the seccomp filter **must** be registered last, immediately
/// before the process is spawned:
///
/// ```text
/// run_as(user)  →  drop_bounding_caps()  →  with_resource_limit(...)  →  with_seccomp(filter)  →  spawn
/// ```
///
/// Rationale:
/// - **`run_as` first**: this temporarily raises `CAP_SETGID` (via
///   [`CapHandle`]) to call `setgroups`, then drops the handle to reclaim the
///   capability, and performs `setuid`/`setgid`. All of these rely on
///   `prctl`/`capset`/`setuid` syscalls that a user-supplied profile could
///   otherwise block, and the bounding set must still contain `CAP_SETGID`.
/// - **`drop_bounding_caps` next**: clears the capability bounding set so the
///   user program can never obtain any capability. It needs `CAP_SETPCAP`
///   (raised temporarily) and therefore runs after `run_as` but before the
///   filter could block `prctl(PR_CAPBSET_DROP)`.
/// - **`with_resource_limit` late**: the judge only *lowers* rlimits, which is
///   unprivileged; placing it just before seccomp keeps the resource
///   constraints applied as close to `execve` as possible.
/// - **`with_seccomp` last**: load the filter right before `execve` so the
///   trusted setup code above runs unconstrained, and the user program starts
///   fully sandboxed. Loading seccomp also sets `PR_SET_NO_NEW_PRIVS`, which
///   takes effect only after privilege manipulation has completed.
pub trait WithSeccomp {
    /// Load the given [`SeccompFilter`] in the child process before `exec`.
    ///
    /// Call this **after**
    /// [`WithResourceLimit`](crate::utils::resources::WithResourceLimit)
    /// and [`RunAs::run_as`].
    fn with_seccomp(&mut self, filter: SeccompFilter) -> &mut Self;
}

impl WithSeccomp for Command {
    fn with_seccomp(&mut self, filter: SeccompFilter) -> &mut Self {
        let load = move || -> std::io::Result<()> { filter.load() };

        // SAFETY: `SeccompFilter::load` performs only async-signal-safe
        // `prctl` calls.
        unsafe { self.pre_exec(load) };

        self
    }
}

#[cfg(test)]
mod tests {
    use capctl::caps::CapSet;
    use serde_json::json;
    use tokio::process::Command;

    use super::*;
    use crate::SeccompConfig;

    #[tokio::test]
    async fn with_seccomp_loads_allow_filter() {
        // A permissive profile: default action is ALLOW, so the child can
        // run normally. This exercises the full `with_seccomp` -> pre_exec
        // -> prctl pipeline.
        let config: SeccompConfig = serde_json::from_value(json!({
            "defaultAction": { "action": "SCMP_ACT_ALLOW" },
        }))
        .unwrap();

        let filter = SeccompFilter::from_config(&config, &CapSet::empty()).unwrap();

        let mut cmd = Command::new("echo");
        cmd.arg("hello");
        cmd.with_seccomp(filter);

        let output = cmd.output().await.expect("failed to spawn");
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout), "hello\n");
    }

    #[tokio::test]
    async fn drop_bounding_caps_clears_bounding_set() {
        // Dropping the bounding set requires CAP_SETPCAP in the permitted
        // set; run the test only when the environment provides it.
        let state = capctl::CapState::get_current().unwrap();
        if !state.permitted.has(Cap::SETPCAP) {
            eprintln!("skipping: CAP_SETPCAP not in permitted set");
            return;
        }

        let mut cmd = Command::new("grep");
        cmd.arg("CapBnd").arg("/proc/self/status");
        cmd.drop_bounding_caps()
            .expect("failed to register bounding drop");

        let output = cmd.output().await.expect("failed to spawn");
        assert!(output.status.success());

        // The bounding set should be all zeros.
        let line = String::from_utf8_lossy(&output.stdout);
        assert!(
            line.contains("0000000000000000"),
            "bounding set was not cleared: {line}"
        );
    }
}
