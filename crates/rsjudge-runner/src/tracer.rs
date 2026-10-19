// SPDX-License-Identifier: Apache-2.0

//! File-access supervision built on `SCMP_ACT_TRACE` and `ptrace`.
//!
//! The seccomp filter generated from a [`SeccompConfig`] that carries a
//! [`FilePolicy`](crate::config::FilePolicy) routes the path-taking syscalls
//! listed in [`FILE_ACCESS_SYSCALLS`] through `SCMP_ACT_TRACE`. The kernel
//! then stops the process on every such syscall and hands control to this
//! module, which reads the requested path out of the stopped process and
//! applies the policy to it.
//!
//! This module describes the traced syscalls, derives the access each of them
//! requests, and supervises the children the filter traces.

use std::io;
use std::os::unix::process::ExitStatusExt as _;
use std::process::{Command as StdCommand, ExitStatus};

use capctl::CapSet;
use nix::sys::ptrace::{self, Event, Options};
use nix::sys::signal::Signal;
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
use nix::unistd::Pid;

use crate::config::{AccessMode, SeccompConfig, SeccompFilter};
use crate::error::Result;
use crate::handler::Handler;

/// A syscall whose path argument the supervisor inspects.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FileAccessSyscall {
    /// The syscall name as understood by libseccomp.
    pub(crate) name: &'static str,

    /// Position of the path argument.
    pub(crate) path_arg: usize,

    /// Position of the directory file descriptor, for the syscalls that take
    /// one. The remaining syscalls resolve relative paths against the current
    /// working directory.
    pub(crate) dirfd_arg: Option<usize>,

    /// How the access requested by the syscall is derived.
    pub(crate) access: AccessArg,
}

/// How the access a traced syscall requests is derived from its arguments.
#[derive(Debug, Clone, Copy)]
pub(crate) enum AccessArg {
    /// The syscall reads file metadata.
    Read,

    /// The syscall creates a file.
    Write,

    /// The syscall opens a file: the `O_*` flags live at this argument index.
    OpenFlags(usize),

    /// The syscall opens a file: the `open_how` structure lives at this
    /// argument index and its leading `flags` field carries the `O_*` bits.
    OpenHow(usize),

    /// The syscall checks accessibility: the `R_OK`/`W_OK`/`X_OK` bits live
    /// at this argument index.
    AccessBits(usize),
}

/// The syscalls the supervisor inspects, in an order that mirrors the file
/// access lifetime: opening, checking, reading metadata and reading links.
pub(crate) const FILE_ACCESS_SYSCALLS: &[FileAccessSyscall] = &[
    FileAccessSyscall {
        name: "open",
        path_arg: 0,
        dirfd_arg: None,
        access: AccessArg::OpenFlags(1),
    },
    FileAccessSyscall {
        name: "openat",
        path_arg: 1,
        dirfd_arg: Some(0),
        access: AccessArg::OpenFlags(2),
    },
    FileAccessSyscall {
        name: "openat2",
        path_arg: 1,
        dirfd_arg: Some(0),
        access: AccessArg::OpenHow(2),
    },
    FileAccessSyscall {
        name: "creat",
        path_arg: 0,
        dirfd_arg: None,
        access: AccessArg::Write,
    },
    FileAccessSyscall {
        name: "access",
        path_arg: 0,
        dirfd_arg: None,
        access: AccessArg::AccessBits(1),
    },
    FileAccessSyscall {
        name: "faccessat",
        path_arg: 1,
        dirfd_arg: Some(0),
        access: AccessArg::AccessBits(2),
    },
    FileAccessSyscall {
        name: "faccessat2",
        path_arg: 1,
        dirfd_arg: Some(0),
        access: AccessArg::AccessBits(2),
    },
    FileAccessSyscall {
        name: "stat",
        path_arg: 0,
        dirfd_arg: None,
        access: AccessArg::Read,
    },
    FileAccessSyscall {
        name: "lstat",
        path_arg: 0,
        dirfd_arg: None,
        access: AccessArg::Read,
    },
    FileAccessSyscall {
        name: "newfstatat",
        path_arg: 1,
        dirfd_arg: Some(0),
        access: AccessArg::Read,
    },
    FileAccessSyscall {
        name: "statx",
        path_arg: 1,
        dirfd_arg: Some(0),
        access: AccessArg::Read,
    },
    FileAccessSyscall {
        name: "readlink",
        path_arg: 0,
        dirfd_arg: None,
        access: AccessArg::Read,
    },
    FileAccessSyscall {
        name: "readlinkat",
        path_arg: 1,
        dirfd_arg: Some(0),
        access: AccessArg::Read,
    },
];

/// The names of the syscalls routed through `SCMP_ACT_TRACE`.
pub(crate) fn file_access_syscall_names() -> impl Iterator<Item = &'static str> {
    FILE_ACCESS_SYSCALLS.iter().map(|syscall| syscall.name)
}

/// The `O_ACCMODE` bits of an `open`-family flags word.
const O_ACCMODE: u64 = 0o3;

/// `open` a file for writing only.
const O_WRONLY: u64 = 0o1;

/// `open` a file for reading and writing.
const O_RDWR: u64 = 0o2;

/// `open` creates the file when it does not exist.
const O_CREAT: u64 = 0o100;

/// `open` creates an unnamed temporary file.
const O_TMPFILE: u64 = 0o20200000;

/// `access` checks for read permission.
const R_OK: u64 = 4;

/// `access` checks for write permission.
const W_OK: u64 = 2;

/// `access` checks for execute permission.
const X_OK: u64 = 1;

/// Derive the access an `open`-family flags word requests.
///
/// `O_RDWR` asks for both kinds of access, and `O_CREAT`/`O_TMPFILE` turn an
/// otherwise read-only open into one that writes (the created file).
#[must_use]
pub(crate) fn access_from_open_flags(flags: u64) -> AccessMode {
    let accmode = flags & O_ACCMODE;
    let creates = flags & (O_CREAT | O_TMPFILE) != 0;

    let reads = accmode != O_WRONLY;
    let writes = accmode != 0 || creates;

    match (reads, writes) {
        (true, true) => AccessMode::Any,
        (true, false) => AccessMode::Read,
        (false, true) => AccessMode::Write,
        (false, false) => AccessMode::Read,
    }
}

/// Derive the access an `access`-family mode word requests.
///
/// `F_OK` (a zero mode) asks whether the path exists at all, which the policy
/// treats as a read.
#[must_use]
pub(crate) fn access_from_access_bits(bits: u64) -> AccessMode {
    let reads = bits & (R_OK | X_OK) != 0 || bits == 0;
    let writes = bits & W_OK != 0;

    match (reads, writes) {
        (true, true) => AccessMode::Any,
        (true, false) => AccessMode::Read,
        (false, true) => AccessMode::Write,
        (false, false) => AccessMode::Read,
    }
}

/// The ptrace options the supervisor attaches a child with.
///
/// `EXITKILL` makes the kernel kill the tracee if the supervisor dies, so a
/// crashed supervisor leaves no submission running. `TRACESYSGOOD` marks
/// system-call stops so they stay distinguishable from ordinary `SIGTRAP`,
/// `TRACESECCOMP` delivers the stops the seccomp filter requests, and
/// `TRACEEXEC` reports the `execve` that starts the submission.
fn attach_options() -> Options {
    Options::PTRACE_O_EXITKILL
        | Options::PTRACE_O_TRACESYSGOOD
        | Options::PTRACE_O_TRACESECCOMP
        | Options::PTRACE_O_TRACEEXEC
}

/// A supervisor holding the sandbox of a child that has yet to be spawned.
///
/// The supervisor is the thread that spawns and serves the child: the child
/// attaches to it with `PTRACE_TRACEME`, and the waits that serve the stops
/// come from that same thread.
///
/// ```no_run
/// # use std::process::Command;
/// # use rsjudge_runner::{SeccompConfig, Tracer};
/// # fn run(config: &SeccompConfig) -> rsjudge_runner::Result<()> {
/// let tracer = Tracer::new(config)?;
/// let mut cmd = Command::new("/usr/bin/cat");
///
/// match tracer {
///     Some(tracer) => {
///         let mut child = tracer.spawn(&mut cmd)?;
///         child.wait()?;
///     }
///     None => {
///         cmd.spawn()?.wait()?;
///     }
/// }
/// # Ok(())
/// # }
/// ```
pub struct Tracer {
    /// The filter the child installs.
    filter: SeccompFilter,
}

impl Tracer {
    /// Prepare supervision for `config`.
    ///
    /// Returns `None` when the profile carries no file-access policy, leaving
    /// the sandbox to run without a supervisor.
    ///
    /// The filter is compiled here, in the spawning process, and inherited by
    /// the child through `fork`: libseccomp allocates while it builds, and the
    /// child's `pre_exec` hook keeps to async-signal-safe syscalls.
    ///
    /// # Errors
    ///
    /// Returns an error when the profile cannot be compiled, which keeps a
    /// submission from running without the policy its profile asks for.
    pub fn new(config: &SeccompConfig) -> Result<Option<Self>> {
        if config.file_policy.is_none() {
            return Ok(None);
        }

        // The filter is loaded by the unprivileged child, so the `includes`
        // and `excludes` matchers see an empty capability set.
        let filter = SeccompFilter::from_config(config, &CapSet::empty())?;

        Ok(Some(Self { filter }))
    }

    /// Spawn `cmd` and take over its supervision.
    ///
    /// The child attaches to this supervisor and then installs the filter, so
    /// the supervisor is in place before the filter routes its first syscall.
    /// The child stops on the `execve` that starts the submission, and the
    /// supervisor enables the stops the filter requests before resuming it —
    /// the new program therefore runs its first syscall under supervision.
    ///
    /// # Errors
    ///
    /// Returns an error when the child cannot be spawned, and when it leaves
    /// before reporting its first stop — in which case the failing `pre_exec`
    /// hook has already kept the program from running.
    pub fn spawn(self, cmd: &mut StdCommand) -> Result<TracedChild> {
        Handler::register(cmd, Handler::supervised(self.filter));

        let child = cmd.spawn()?;
        let pid = Pid::from_raw(child.id() as i32);

        ptrace::setoptions(pid, attach_options()).map_err(io::Error::other)?;

        Ok(TracedChild::new(pid, child))
    }
}

/// A child process running under supervision.
pub struct TracedChild {
    /// The process id of the child.
    pid: Pid,

    /// The child handle.
    child: std::process::Child,

    /// Whether the child has yet to report the stop that follows its own
    /// `execve`.
    exec_trap_pending: bool,

    /// The number of file-access stops served so far.
    stops: usize,
}

impl TracedChild {
    /// The process id of the child.
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// The child handle, for standard input, output and error.
    ///
    /// The supervisor owns the child's lifetime events, so reach the process
    /// through [`wait`](Self::wait) and [`try_wait`](Self::try_wait) instead
    /// of waiting on this handle.
    pub fn child(&self) -> &std::process::Child {
        &self.child
    }

    /// The number of file-access stops served so far.
    pub fn stops(&self) -> usize {
        self.stops
    }

    /// Serve the stops that have arrived so far.
    ///
    /// # Errors
    ///
    /// Returns an error when waiting for the child fails.
    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
        loop {
            let status = waitpid(self.pid, Some(WaitPidFlag::WNOHANG)).map_err(io::Error::other)?;

            if status == WaitStatus::StillAlive {
                return Ok(None);
            }
            if let Some(exit) = self.serve(status)? {
                return Ok(Some(exit));
            }
        }
    }

    /// Serve the child's stops until it exits, and report the exit status.
    ///
    /// # Errors
    ///
    /// Returns an error when waiting for the child fails.
    pub fn wait(&mut self) -> Result<ExitStatus> {
        loop {
            if let Some(status) = self.serve(waitpid(self.pid, None).map_err(io::Error::other)?)? {
                return Ok(status);
            }
        }
    }

    /// Signal the child to terminate.
    ///
    /// # Errors
    ///
    /// Returns an error when the signal cannot be delivered.
    pub fn kill(&mut self) -> Result<()> {
        Ok(self.child.kill()?)
    }

    /// A supervisor for `child`.
    fn new(pid: Pid, child: std::process::Child) -> Self {
        Self {
            pid,
            child,
            exec_trap_pending: true,
            stops: 0,
        }
    }

    /// Serve one wait status, resuming the child unless it has exited.
    fn serve(&mut self, status: WaitStatus) -> Result<Option<ExitStatus>> {
        match status {
            WaitStatus::Exited(_, code) => Ok(Some(ExitStatus::from_raw(code << 8))),
            WaitStatus::Signaled(_, signal, dumped) => {
                Ok(Some(ExitStatus::from_raw(exit_signal(signal, dumped))))
            }
            WaitStatus::Stopped(pid, signal) => {
                // The kernel reports the `execve` that starts the submission
                // as a `SIGTRAP` stop, handing the supervisor control before
                // the new program runs; that signal is the notification
                // itself, so injecting it would end the submission.
                let exec_trap =
                    std::mem::take(&mut self.exec_trap_pending) && signal == Signal::SIGTRAP;

                // A stop from a suspending signal is resumed without it, so a
                // submission cannot halt itself under the supervisor, while
                // every other signal reaches the program unchanged.
                let deliver = (!exec_trap && !suspends(signal)).then_some(signal);

                ptrace::cont(pid, deliver).map_err(io::Error::other)?;
                Ok(None)
            }
            WaitStatus::PtraceEvent(pid, _, event) => {
                self.exec_trap_pending = false;
                self.event(pid, event)?;
                Ok(None)
            }
            WaitStatus::PtraceSyscall(pid) => {
                self.exec_trap_pending = false;
                ptrace::cont(pid, None).map_err(io::Error::other)?;
                Ok(None)
            }
            WaitStatus::Continued(_) | WaitStatus::StillAlive => Ok(None),
        }
    }

    /// Serve a `PTRACE_EVENT_*` stop and resume the child.
    fn event(&mut self, pid: Pid, event: i32) -> Result<()> {
        if event == Event::PTRACE_EVENT_SECCOMP as i32 {
            self.stops += 1;
            log::trace!("file-access stop on {pid}");
        }

        ptrace::cont(pid, None).map_err(io::Error::other)?;

        Ok(())
    }
}

/// Whether `signal` suspends the process group rather than reporting an event.
fn suspends(signal: Signal) -> bool {
    matches!(
        signal,
        Signal::SIGSTOP | Signal::SIGTSTP | Signal::SIGTTIN | Signal::SIGTTOU
    )
}

/// The raw wait status of a process that left through `signal`.
fn exit_signal(signal: Signal, dumped: bool) -> i32 {
    signal as i32 | if dumped { 0x80 } else { 0 }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use serde_json::json;

    use super::*;

    /// A profile that leaves ordinary syscalls alone and routes file access
    /// through the supervisor.
    fn traced_config() -> SeccompConfig {
        serde_json::from_value(json!({
            "defaultAction": { "action": "SCMP_ACT_ALLOW" },
            "filePolicy": { "defaultAction": "deny" },
        }))
        .expect("failed to parse")
    }

    #[test]
    fn tracer_requires_a_file_policy() {
        let config: SeccompConfig = serde_json::from_value(json!({
            "defaultAction": { "action": "SCMP_ACT_ALLOW" },
        }))
        .expect("failed to parse");

        assert!(Tracer::new(&config).unwrap().is_none());
        assert!(Tracer::new(&traced_config()).unwrap().is_some());
    }

    #[test]
    fn supervised_child_runs_through_its_stops() {
        let config = traced_config();

        // The profile must reach the trace rules: a filter that fails to
        // compile would leave the child unsupervised.
        SeccompFilter::from_config(&config, &CapSet::empty()).expect("filter did not compile");

        let tracer = Tracer::new(&config)
            .expect("failed to prepare the tracer")
            .expect("no tracer for a traced profile");

        let mut cmd = Command::new("echo");
        cmd.arg("hello");

        let mut child = tracer.spawn(&mut cmd).expect("failed to spawn");

        let status = child.wait().expect("failed to supervise the child");
        assert!(status.success(), "the child exited with {status:?}");

        // The supervisor only resumes the child on each stop, so a child that
        // exits cleanly proves the handshake and the attach worked.
        assert!(
            child.stops() > 0,
            "the child never reached a traced syscall"
        );
    }

    #[test]
    fn try_wait_polls_a_running_child_and_kill_stops_it() {
        let tracer = Tracer::new(&traced_config()).unwrap().unwrap();

        let mut cmd = Command::new("sleep");
        cmd.arg("30");

        let mut child = tracer.spawn(&mut cmd).expect("failed to spawn");
        assert!(child.try_wait().expect("failed to poll").is_none());

        child.kill().expect("failed to signal the child");

        let status = child.wait().expect("failed to supervise");
        assert!(!status.success(), "the child outlived its signal");
    }

    #[test]
    fn open_flags_map_to_access() {
        // `O_RDONLY` is zero, so a plain read open asks for read access.
        assert_eq!(access_from_open_flags(0), AccessMode::Read);
        assert_eq!(access_from_open_flags(O_WRONLY), AccessMode::Write);
        assert_eq!(access_from_open_flags(O_RDWR), AccessMode::Any);

        // Creating the file writes to it even when it is opened read-only.
        assert_eq!(access_from_open_flags(O_CREAT), AccessMode::Any);
        assert_eq!(
            access_from_open_flags(O_WRONLY | O_CREAT),
            AccessMode::Write
        );
        assert_eq!(access_from_open_flags(O_TMPFILE), AccessMode::Any);

        // `O_PATH` performs no I/O and is a read.
        assert_eq!(access_from_open_flags(0o10000000), AccessMode::Read);
    }

    #[test]
    fn access_bits_map_to_access() {
        // A zero mode is `F_OK`, an existence check.
        assert_eq!(access_from_access_bits(0), AccessMode::Read);
        assert_eq!(access_from_access_bits(R_OK), AccessMode::Read);
        assert_eq!(access_from_access_bits(X_OK), AccessMode::Read);
        assert_eq!(access_from_access_bits(W_OK), AccessMode::Write);
        assert_eq!(access_from_access_bits(R_OK | W_OK), AccessMode::Any);
    }

    #[test]
    fn every_traced_syscall_takes_the_argument_indices_it_declares() {
        for syscall in FILE_ACCESS_SYSCALLS {
            if let Some(dirfd) = syscall.dirfd_arg {
                assert!(
                    dirfd < syscall.path_arg,
                    "{} declares dirfd at {dirfd} and path at {}",
                    syscall.name,
                    syscall.path_arg,
                );
            }

            // The flags and mode words follow the path in every traced
            // syscall that takes one.
            let access_arg = match syscall.access {
                AccessArg::OpenFlags(arg)
                | AccessArg::OpenHow(arg)
                | AccessArg::AccessBits(arg) => Some(arg),
                AccessArg::Read | AccessArg::Write => None,
            };
            if let Some(arg) = access_arg {
                assert!(
                    arg > syscall.path_arg,
                    "{} declares its access word at {arg} and path at {}",
                    syscall.name,
                    syscall.path_arg,
                );
            }
        }
    }
}
