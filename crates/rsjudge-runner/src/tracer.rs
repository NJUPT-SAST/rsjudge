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

use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::{self, IoSliceMut};
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::process::ExitStatusExt as _;
use std::path::{Component, Path, PathBuf};
use std::process::{Command as StdCommand, ExitStatus};

use capctl::CapSet;
use libseccomp::ScmpSyscall;
use nix::sys::ptrace::{self, Event, Options};
use nix::sys::signal::{Signal, kill};
use nix::sys::uio::{RemoteIoVec, process_vm_readv};
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
use nix::unistd::Pid;

use crate::config::{AccessMode, FileAction, FilePolicy, SeccompConfig, SeccompFilter};
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

/// The syscall requests a write only.
const O_WRONLY: u64 = 0o1;

/// The syscall requests a read and a write.
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

    let reads = matches!(accmode, 0 | O_RDWR);
    let writes = matches!(accmode, O_WRONLY | O_RDWR) || creates;

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

/// The longest path the supervisor reads out of a stopped process.
const PATH_MAX: usize = 4096;

/// The length of a read that walks a string out of the stopped process.
const READ_CHUNK: usize = 256;

/// The `AT_FDCWD` directory file descriptor, which names the current working
/// directory rather than an open descriptor.
const AT_FDCWD: i32 = -100;

/// The syscall a stopped process is about to enter.
struct Syscall {
    /// The syscall number.
    number: i32,

    /// The six syscall arguments.
    args: [u64; 6],
}

impl Syscall {
    /// The syscall the registers describe.
    fn from_regs(regs: &nix::libc::user_regs_struct) -> Self {
        Self {
            number: syscall_number(regs),
            args: syscall_args(regs),
        }
    }

    /// The argument at `index`.
    fn arg(&self, index: usize) -> u64 {
        self.args[index]
    }
}

/// The base a syscall resolves its relative path against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathBase {
    /// The working directory of the child.
    Cwd,

    /// An open descriptor of the child.
    Descriptor(i32),
}

impl PathBase {
    /// The base the traced syscall takes: the descriptor it carries, or the
    /// working directory when it carries none or takes `AT_FDCWD`.
    fn of(dirfd_arg: Option<usize>, syscall: &Syscall) -> Self {
        match dirfd_arg.map(|arg| syscall.arg(arg) as i32) {
            Some(dirfd) if dirfd != AT_FDCWD => Self::Descriptor(dirfd),
            _ => Self::Cwd,
        }
    }
}

/// Read the number of the syscall the registers hold.
#[cfg(target_arch = "x86_64")]
fn syscall_number(regs: &nix::libc::user_regs_struct) -> i32 {
    regs.orig_rax as i32
}

/// Read the six syscall arguments out of the registers.
#[cfg(target_arch = "x86_64")]
fn syscall_args(regs: &nix::libc::user_regs_struct) -> [u64; 6] {
    [regs.rdi, regs.rsi, regs.rdx, regs.r10, regs.r8, regs.r9]
}

/// Replace the syscall number with an invalid one, which fails the syscall
/// with `ENOSYS` once the process resumes.
#[cfg(target_arch = "x86_64")]
fn cancel_syscall(regs: &mut nix::libc::user_regs_struct) {
    regs.orig_rax = u64::MAX;
}

/// Read the number of the syscall the registers hold.
#[cfg(target_arch = "aarch64")]
fn syscall_number(regs: &nix::libc::user_regs_struct) -> i32 {
    regs.regs[8] as i32
}

/// Read the six syscall arguments out of the registers.
#[cfg(target_arch = "aarch64")]
fn syscall_args(regs: &nix::libc::user_regs_struct) -> [u64; 6] {
    [
        regs.regs[0],
        regs.regs[1],
        regs.regs[2],
        regs.regs[3],
        regs.regs[4],
        regs.regs[5],
    ]
}

/// Replace the syscall number with an invalid one, which fails the syscall
/// with `ENOSYS` once the process resumes.
#[cfg(target_arch = "aarch64")]
fn cancel_syscall(regs: &mut nix::libc::user_regs_struct) {
    regs.regs[8] = u64::MAX;
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
compile_error!("file-access supervision reads syscall registers on x86_64 and aarch64");

/// Read a NUL-terminated string out of the memory of a stopped process.
///
/// The supervisor walks the string in chunks so a long path costs no more
/// memory than the path itself, and stops at `limit` bytes.
fn read_cstring(pid: Pid, address: usize, limit: usize) -> io::Result<Vec<u8>> {
    if address == 0 {
        // A null pointer names no path at all, which the policy treats like
        // an unresolvable one.
        return Err(io::Error::other("the path argument is null"));
    }

    let mut path = Vec::new();
    while path.len() < limit {
        let mut chunk = [0u8; READ_CHUNK];
        let take = chunk.len().min(limit - path.len());

        let mut local = [IoSliceMut::new(&mut chunk[..take])];
        let remote = [RemoteIoVec {
            base: address + path.len(),
            len: take,
        }];

        let read = process_vm_readv(pid, &mut local, &remote).map_err(io::Error::other)?;
        if read == 0 {
            break;
        }

        match chunk[..read].iter().position(|byte| *byte == 0) {
            Some(end) => {
                path.extend_from_slice(&chunk[..end]);
                return Ok(path);
            }
            None => path.extend_from_slice(&chunk[..read]),
        }
    }

    Err(io::Error::other("the path is unterminated"))
}

/// Read a `u64` out of the memory of a stopped process.
fn read_word(pid: Pid, address: usize) -> io::Result<u64> {
    let mut word = [0u8; size_of::<u64>()];
    let len = word.len();

    {
        let mut local = [IoSliceMut::new(&mut word)];
        let remote = [RemoteIoVec { base: address, len }];

        process_vm_readv(pid, &mut local, &remote).map_err(io::Error::other)?;
    }

    Ok(u64::from_ne_bytes(word))
}

/// Resolve a path against `.` and `..` without touching the file system.
///
/// A path a program builds from `.` and `..` names the same file as its
/// resolved form, and resolving it keeps a rule from matching a path that only
/// resembles the location it covers.
fn resolve(path: &Path) -> PathBuf {
    let mut resolved = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // A path above the root stays at the root.
                if !resolved.pop() {
                    resolved.push(Component::RootDir);
                }
            }
            component => resolved.push(component),
        }
    }

    resolved
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

    /// The file-access policy the supervisor applies.
    policy: FilePolicy,

    /// The traced syscalls, keyed by syscall number.
    syscalls: HashMap<i32, &'static FileAccessSyscall>,
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
        let Some(policy) = config.file_policy.clone() else {
            return Ok(None);
        };

        // The filter is loaded by the unprivileged child, so the `includes`
        // and `excludes` matchers see an empty capability set.
        let filter = SeccompFilter::from_config(config, &CapSet::empty())?;

        Ok(Some(Self {
            filter,
            policy,
            syscalls: syscall_table(),
        }))
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
        let Self {
            filter,
            policy,
            syscalls,
        } = self;

        Handler::register(cmd, Handler::supervised(filter));

        let child = cmd.spawn()?;
        let pid = Pid::from_raw(child.id() as i32);

        // The child stops on the `execve` that starts the submission. Waiting
        // for that stop gives `setoptions` a stopped tracee, which the kernel
        // asks for, and the stop itself carries no signal to deliver, so
        // resuming without one starts the submission under the options.
        waitpid(pid, None).map_err(io::Error::other)?;

        ptrace::setoptions(pid, attach_options()).map_err(io::Error::other)?;
        ptrace::cont(pid, None).map_err(io::Error::other)?;

        Ok(TracedChild::new(pid, child, policy, syscalls))
    }
}

/// The traced syscalls, keyed by the number the kernel uses for them.
fn syscall_table() -> HashMap<i32, &'static FileAccessSyscall> {
    FILE_ACCESS_SYSCALLS
        .iter()
        .filter_map(|syscall| {
            // A syscall this architecture lacks carries no number here.
            ScmpSyscall::from_name(syscall.name)
                .ok()
                .map(|number| (number.as_raw_syscall(), syscall))
        })
        .collect()
}

/// A child process running under supervision.
pub struct TracedChild {
    /// The process id of the child.
    pid: Pid,

    /// The child handle.
    child: std::process::Child,

    /// The file-access policy the supervisor applies.
    policy: FilePolicy,

    /// The traced syscalls, keyed by syscall number.
    syscalls: HashMap<i32, &'static FileAccessSyscall>,

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
    fn new(
        pid: Pid,
        child: std::process::Child,
        policy: FilePolicy,
        syscalls: HashMap<i32, &'static FileAccessSyscall>,
    ) -> Self {
        Self {
            pid,
            child,
            policy,
            syscalls,
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
                // The supervisor consumed the exec stop while spawning the
                // child, so this stop reports a signal the program raised. A
                // suspending signal is dropped so a submission stays under
                // supervision, while every other one reaches the program.
                let deliver = (!suspends(signal)).then_some(signal);

                ptrace::cont(pid, deliver).map_err(io::Error::other)?;
                Ok(None)
            }
            WaitStatus::PtraceEvent(pid, _, event) => {
                self.event(pid, event)?;
                Ok(None)
            }
            WaitStatus::PtraceSyscall(pid) => {
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
            return self.enforce(pid);
        }

        ptrace::cont(pid, None).map_err(io::Error::other)?;

        Ok(())
    }

    /// Apply the file-access policy to the syscall the child is entering.
    ///
    /// The policy is closed by default, so a path the supervisor cannot read
    /// fails the syscall rather than slipping past the rules.
    fn enforce(&self, pid: Pid) -> Result<()> {
        let regs = ptrace::getregs(pid).map_err(io::Error::other)?;
        let syscall = Syscall::from_regs(&regs);

        let Some(entry) = self.syscalls.get(&syscall.number) else {
            // A traced syscall carries no path rule of its own, so it runs.
            return cont(pid);
        };

        let action = match self.verdict(pid, entry, &syscall) {
            Ok(action) => action,
            Err(err) => {
                log::warn!("reading the path of {} failed: {err}", entry.name);
                FileAction::Deny
            }
        };

        match action {
            FileAction::Allow => cont(pid),
            FileAction::Deny => {
                let mut regs = regs;
                cancel_syscall(&mut regs);
                ptrace::setregs(pid, regs).map_err(io::Error::other)?;
                cont(pid)
            }
            FileAction::Kill => {
                log::warn!("killing {pid} for entering a path its policy forbids");
                kill(pid, Signal::SIGKILL).map_err(io::Error::other)?;
                // The signal ends the process, so a refusal here is expected.
                let _ = ptrace::cont(pid, None);
                Ok(())
            }
        }
    }

    /// The action the policy takes on the path the syscall requests.
    fn verdict(
        &self,
        pid: Pid,
        entry: &FileAccessSyscall,
        syscall: &Syscall,
    ) -> io::Result<FileAction> {
        let path = self.requested_path(pid, entry, syscall)?;
        let access = self.requested_access(pid, entry, syscall)?;

        Ok(self.policy.evaluate(&path, access))
    }

    /// The path the syscall requests, resolved against the child's working
    /// directory or the descriptor it opens from.
    fn requested_path(
        &self,
        pid: Pid,
        entry: &FileAccessSyscall,
        syscall: &Syscall,
    ) -> io::Result<PathBuf> {
        let raw = read_cstring(pid, syscall.arg(entry.path_arg) as usize, PATH_MAX)?;
        let path = Path::new(OsStr::from_bytes(&raw));

        // An absolute path names the file on its own.
        if path.is_absolute() {
            return Ok(resolve(path));
        }

        // A relative path resolves against the descriptor the syscall takes,
        // or against the working directory of the child, so the supervisor
        // reads the path in the terms the child used.
        let base = match PathBase::of(entry.dirfd_arg, syscall) {
            PathBase::Cwd => self.child_cwd(pid)?,
            PathBase::Descriptor(dirfd) => self.child_fd(pid, dirfd)?,
        };

        Ok(resolve(&base.join(path)))
    }

    /// The access the syscall requests.
    fn requested_access(
        &self,
        pid: Pid,
        entry: &FileAccessSyscall,
        syscall: &Syscall,
    ) -> io::Result<AccessMode> {
        Ok(match entry.access {
            AccessArg::Read => AccessMode::Read,
            AccessArg::Write => AccessMode::Write,
            AccessArg::OpenFlags(index) => access_from_open_flags(syscall.arg(index)),
            AccessArg::OpenHow(index) => {
                // The `open_how` structure leads with its flags word.
                access_from_open_flags(read_word(pid, syscall.arg(index) as usize)?)
            }
            AccessArg::AccessBits(index) => access_from_access_bits(syscall.arg(index)),
        })
    }

    /// The working directory of the child.
    fn child_cwd(&self, pid: Pid) -> io::Result<PathBuf> {
        std::fs::read_link(format!("/proc/{pid}/cwd")).map_err(io::Error::other)
    }

    /// The directory the child's `dirfd` names.
    fn child_fd(&self, pid: Pid, dirfd: i32) -> io::Result<PathBuf> {
        std::fs::read_link(format!("/proc/{pid}/fd/{dirfd}")).map_err(io::Error::other)
    }
}

/// Resume the child without a signal.
fn cont(pid: Pid) -> Result<()> {
    ptrace::cont(pid, None).map_err(io::Error::other)?;

    Ok(())
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use serde_json::json;

    use super::*;

    /// A profile that leaves ordinary syscalls alone and routes file access
    /// through the supervisor.
    fn traced_config(policy: serde_json::Value) -> SeccompConfig {
        serde_json::from_value(json!({
            "defaultAction": { "action": "SCMP_ACT_ALLOW" },
            "filePolicy": policy,
        }))
        .expect("failed to parse")
    }

    /// A policy that admits the read access a dynamically linked program
    /// needs to start.
    fn permissive_policy() -> serde_json::Value {
        json!({
            "defaultAction": "deny",
            "rules": [
                {
                    "paths": ["/"],
                    "match": "prefix",
                    "access": "any",
                    "action": "allow",
                },
            ],
        })
    }

    #[test]
    fn tracer_requires_a_file_policy() {
        let config: SeccompConfig = serde_json::from_value(json!({
            "defaultAction": { "action": "SCMP_ACT_ALLOW" },
        }))
        .expect("failed to parse");

        assert!(Tracer::new(&config).unwrap().is_none());
        assert!(
            Tracer::new(&traced_config(permissive_policy()))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn supervised_child_runs_through_its_stops() {
        let config = traced_config(permissive_policy());

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

        // Every intercepted syscall is served and resumed, and a program that
        // starts at all reaches the loader's file access.
        assert!(
            child.stops() > 0,
            "the child never reached a traced syscall"
        );
    }

    #[test]
    fn default_deny_fails_the_file_access_of_a_program() {
        let config = traced_config(json!({ "defaultAction": "deny" }));

        let tracer = Tracer::new(&config).unwrap().unwrap();
        let mut cmd = Command::new("echo");
        cmd.arg("hello");

        let mut child = tracer.spawn(&mut cmd).expect("failed to spawn");
        let status = child.wait().expect("failed to supervise");

        // The loader cannot reach the libraries it needs, so the program stops
        // with a failure rather than running under an open policy.
        assert!(!status.success(), "the child ran without its file access");
        assert!(child.stops() > 0, "the child reached no traced syscall");
    }

    #[test]
    fn a_kill_rule_ends_the_process_that_reaches_the_path() {
        let config = traced_config(json!({
            "defaultAction": "allow",
            "rules": [
                {
                    "paths": ["/usr"],
                    "match": "prefix",
                    "access": "read",
                    "action": "kill",
                },
            ],
        }));

        let tracer = Tracer::new(&config).unwrap().unwrap();
        let mut cmd = Command::new("echo");
        cmd.arg("hello");

        let mut child = tracer.spawn(&mut cmd).expect("failed to spawn");
        let status = child.wait().expect("failed to supervise");

        // Every dynamically linked program reads under `/usr` for its loader
        // and libraries, so the rule kills the child before it runs.
        assert_eq!(status.signal(), Some(Signal::SIGKILL as i32));
    }

    #[test]
    fn a_relative_path_resolves_against_the_working_directory_of_the_child() {
        let scratch = Scratch::new(&[("answer.txt", "hello\n"), ("blocked.txt", "secret\n")]);

        // A permissive profile whose one rule covers a path the child reaches
        // through a relative name.
        let config = traced_config(json!({
            "defaultAction": "allow",
            "rules": [
                { "paths": [scratch.path("blocked.txt")], "action": "deny" },
            ],
        }));

        let allowed = run_in_dir(&config, &scratch, "cat", &["answer.txt"]).0;
        let denied = run_in_dir(&config, &scratch, "cat", &["blocked.txt"]).0;

        // The rule names the file through the working directory of the child,
        // so a supervisor that resolved relative paths in its own terms would
        // miss the rule and read the file.
        assert!(allowed, "the child read a path its policy admits");
        assert!(!denied, "the child read a path its policy denies");
    }

    #[test]
    fn file_policy_matches_prefix_and_exact_paths() {
        let scratch = Scratch::new(&[
            ("answer.txt", "hello\n"),
            ("blocked.txt", "secret\n"),
            ("blocked.txt.bak", "backup\n"),
            ("private/secret.txt", "hidden\n"),
        ]);

        let config = traced_config(json!({
            "defaultAction": "allow",
            "rules": [
                // The first rule covers everything below the directory it
                // names, the second only the single path it names.
                { "paths": [scratch.path("private")], "match": "prefix", "action": "deny" },
                { "paths": [scratch.path("blocked.txt")], "match": "exact", "action": "deny" },
            ],
        }));

        let sibling = run_in_dir(&config, &scratch, "cat", &["blocked.txt.bak"]).0;
        let nested = run_in_dir(&config, &scratch, "cat", &["private/secret.txt"]).0;
        let dot_dot = run_in_dir(&config, &scratch, "cat", &["private/../blocked.txt"]).0;

        // The exact rule leaves a neighbouring name alone, the prefix rule
        // covers a nested path, and the supervisor resolves `.` and `..`
        // before it matches.
        assert!(sibling, "the exact rule covered a neighbouring name");
        assert!(!nested, "the prefix rule missed a nested path");
        assert!(!dot_dot, "a `..` segment escaped the exact rule");
    }

    #[test]
    fn file_policy_distinguishes_read_and_write_access() {
        let scratch = Scratch::new(&[("readonly.txt", "data\n")]);

        // The path takes reads, and writes to it fail.
        let config = traced_config(json!({
            "defaultAction": "allow",
            "rules": [
                {
                    "paths": [scratch.path("readonly.txt")],
                    "access": "write",
                    "action": "deny",
                },
            ],
        }));

        let read = run_in_dir(&config, &scratch, "cat", &["readonly.txt"]).0;
        let write = run_in_dir(
            &config,
            &scratch,
            "sh",
            &["-c", "echo more >> readonly.txt"],
        )
        .0;

        assert!(read, "a read request met a rule that covers writes");
        assert!(!write, "a write request passed a rule that covers writes");
    }

    #[test]
    fn try_wait_reports_the_exit_of_a_supervised_child() {
        let scratch = Scratch::new(&[("answer.txt", "hello\n")]);
        let config = traced_config(permissive_policy());

        let (success, stops) = run_in_dir(&config, &scratch, "echo", &["hello"]);
        assert!(success, "the child failed without a rule to stop it");

        // A program that starts at all reads its cache, its loader and its
        // libraries through more than one traced syscall.
        assert!(stops > 1, "the child reached a single traced syscall");

        // Polling reports the exit of a child that has already left.
        let tracer = Tracer::new(&config).unwrap().unwrap();
        let mut cmd = Command::new("echo");
        cmd.arg("hello");
        cmd.stdout(std::process::Stdio::null());
        let mut child = tracer.spawn(&mut cmd).expect("failed to spawn");

        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = child.try_wait().expect("failed to poll") {
                break status;
            }
            assert!(Instant::now() < deadline, "the child never left");
            std::thread::sleep(Duration::from_millis(1));
        };

        assert!(status.success(), "the child exited with {status:?}");
    }

    /// A scratch directory holding the files a test case reads.
    struct Scratch {
        /// The directory the fixtures live in.
        dir: PathBuf,
    }

    impl Scratch {
        /// A directory holding `files`, each named by its relative path.
        fn new(files: &[(&str, &str)]) -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);

            let serial = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir()
                .join(format!("rsjudge-tracer-{}-{serial}", std::process::id()));
            let scratch = Self { dir };

            for (name, contents) in files {
                let path = scratch.path(name);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).expect("failed to create a fixture directory");
                }
                std::fs::write(path, contents).expect("failed to write a fixture");
            }

            scratch
        }

        /// The path of the fixture that `name` names.
        fn path(&self, name: &str) -> PathBuf {
            self.dir.join(name)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            // Removing the fixtures keeps the temporary directory tidy, and a
            // failure here only leaves files behind.
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// Run `program` from the scratch directory under `config`, and report
    /// whether it succeeded along with the number of traced syscalls it made.
    fn run_in_dir(
        config: &SeccompConfig,
        scratch: &Scratch,
        program: &str,
        args: &[&str],
    ) -> (bool, usize) {
        let tracer = Tracer::new(config)
            .unwrap()
            .expect("no tracer for a traced profile");

        let mut cmd = Command::new(program);
        cmd.args(args);
        cmd.current_dir(&scratch.dir);
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());

        let mut child = tracer.spawn(&mut cmd).expect("failed to spawn");
        let status = child.wait().expect("failed to supervise");

        assert!(child.stops() > 0, "{program} reached no traced syscall");

        (status.success(), child.stops())
    }

    #[test]
    fn a_syscall_resolves_a_relative_path_against_its_directory() {
        // A syscall without a directory descriptor resolves against the
        // working directory, as do the `at` syscalls that take `AT_FDCWD`.
        assert_eq!(PathBase::of(None, &syscall_with_dirfd(7)), PathBase::Cwd);
        assert_eq!(
            PathBase::of(Some(0), &syscall_with_dirfd(AT_FDCWD as u64)),
            PathBase::Cwd
        );

        // Any other descriptor names the directory to resolve against.
        assert_eq!(
            PathBase::of(Some(0), &syscall_with_dirfd(7)),
            PathBase::Descriptor(7)
        );
    }

    /// A syscall that names `dirfd` in its first argument.
    fn syscall_with_dirfd(dirfd: u64) -> Syscall {
        Syscall {
            number: 0,
            args: [dirfd, 0, 0, 0, 0, 0],
        }
    }

    #[test]
    fn resolve_settles_dot_segments_without_touching_the_file_system() {
        // A path that names a file through `.` and `..` matches the rules of
        // the location it settles on.
        assert_eq!(
            resolve(Path::new("/usr/../etc/passwd")),
            PathBuf::from("/etc/passwd")
        );
        assert_eq!(resolve(Path::new("/a/./b/")), PathBuf::from("/a/b"));
        assert_eq!(resolve(Path::new("./relative")), PathBuf::from("relative"));

        // A path that climbs above the root stays at the root.
        assert_eq!(resolve(Path::new("/../../../etc")), PathBuf::from("/etc"));
        assert_eq!(resolve(Path::new("a/../..")), PathBuf::from("/"));
    }

    #[test]
    fn resolve_keeps_the_components_a_prefix_match_relies_on() {
        // A rule on `/etc` covers `/etc/passwd` and leaves `/etcetera` alone,
        // which `Path::starts_with` decides component by component.
        assert!(resolve(Path::new("/etc/passwd")).starts_with("/etc"));
        assert!(!resolve(Path::new("/etcetera")).starts_with("/etc"));
    }

    #[test]
    fn read_cstring_walks_a_string_out_of_a_process() {
        // A stopped child is read through `process_vm_readv`, which the
        // supervisor can exercise against its own memory.
        let text = b"/tmp/example\0trailing";
        let path = read_cstring(Pid::this(), text.as_ptr() as usize, PATH_MAX)
            .expect("failed to read the string");
        assert_eq!(path, b"/tmp/example");

        // A string longer than a single read is walked in chunks.
        let long = format!("/{}", "a".repeat(READ_CHUNK * 2));
        let mut buffer = long.clone().into_bytes();
        buffer.push(0);
        let walked = read_cstring(Pid::this(), buffer.as_ptr() as usize, PATH_MAX)
            .expect("failed to read the long string");
        assert_eq!(walked, long.as_bytes());
    }

    #[test]
    fn read_cstring_refuses_an_argument_the_policy_cannot_judge() {
        // A null pointer names no path at all.
        assert!(read_cstring(Pid::this(), 0, PATH_MAX).is_err());

        // A path with no terminator within the limit stays unread.
        let unterminated = [b'x'; 16];
        assert!(
            read_cstring(Pid::this(), unterminated.as_ptr() as usize, 8).is_err(),
            "an unterminated argument was read"
        );
    }

    #[test]
    fn read_word_reads_a_word_out_of_a_process() {
        let word = 0x0123_4567_89ab_cdef_u64;
        let read = read_word(Pid::this(), (&word as *const u64) as usize)
            .expect("failed to read the word");
        assert_eq!(read, word);
    }

    #[test]
    fn a_profile_that_fails_to_compile_reports_an_error() {
        // A syscall the running architecture lacks keeps the supervisor from
        // building its filter, so the submission stays unimplemented rather
        // than running under a policy that never loaded.
        let config = traced_config(json!({ "defaultAction": "deny" }));
        let mut broken = config.clone();
        broken.syscalls = Some(
            serde_json::from_value(json!([
                { "names": ["no_such_syscall"], "action": "SCMP_ACT_ALLOW" },
            ]))
            .expect("failed to build the rule"),
        );

        assert!(
            Tracer::new(&broken).is_err(),
            "a profile that cannot compile produced a tracer"
        );
    }

    #[test]
    fn try_wait_polls_a_running_child_and_kill_stops_it() {
        let tracer = Tracer::new(&traced_config(permissive_policy()))
            .unwrap()
            .unwrap();

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
