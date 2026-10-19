// SPDX-License-Identifier: Apache-2.0

//! Sandbox installation in the forked child.
//!
//! A child whose profile carries a file-access policy attaches itself to its
//! supervisor ([`Tracer`](crate::Tracer)) **before** installing the filter:
//! the seccomp filter routes the path-taking syscalls through
//! `SCMP_ACT_TRACE`, and the kernel only delivers those stops while the
//! process has a tracer. `ptrace(PTRACE_TRACEME)` performs the attachment from
//! the child, which is what the runner can do here — the fork has yet to hand
//! the parent a process id, and `Command::spawn` returns only once the child
//! has exec'd.
//!
//! The supervisor is the thread that called `Command::spawn`:
//! `PTRACE_TRACEME` names the parent thread as the tracer, and the waits that
//! serve the stops must come from that same thread.

use std::io;
use std::os::unix::process::CommandExt as _;
use std::process::Command as StdCommand;

use nix::sys::ptrace;

use crate::config::SeccompFilter;

/// The sandbox a child installs for itself before `exec`.
pub struct Handler {
    /// The filter to install.
    filter: SeccompFilter,

    /// Whether the child attaches itself to a supervising tracer before
    /// installing the filter.
    attaches: bool,
}

impl Handler {
    /// A sandbox that installs `filter` on its own.
    pub fn seccomp(filter: SeccompFilter) -> Self {
        Self {
            filter,
            attaches: false,
        }
    }

    /// A sandbox whose filter the supervisor is attached to first.
    pub(crate) fn supervised(filter: SeccompFilter) -> Self {
        Self {
            filter,
            attaches: true,
        }
    }

    /// Register the sandbox as a `pre_exec` hook on `cmd`.
    pub(crate) fn register(cmd: &mut StdCommand, handler: Self) {
        let install = move || -> io::Result<()> { handler.install() };

        // SAFETY: `Handler::install` performs only async-signal-safe
        // syscalls: `ptrace`, `prctl` and `seccomp`.
        unsafe { cmd.pre_exec(install) };
    }

    /// Install the sandbox in the calling process.
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` when the attachment fails, and propagates any
    /// error the filter installation raises. Either leaves the child without
    /// a sandbox, so the failing hook stops the child before `exec`.
    fn install(&self) -> io::Result<()> {
        if self.attaches {
            // Attaching before the filter is installed gives the supervisor
            // every stop the filter produces, from the first one on.
            ptrace::traceme().map_err(io::Error::from)?;
        }

        self.filter.load()
    }
}
