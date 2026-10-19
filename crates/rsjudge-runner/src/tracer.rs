// SPDX-License-Identifier: Apache-2.0

//! File-access supervision built on `SCMP_ACT_TRACE` and `ptrace`.
//!
//! The seccomp filter generated from a [`SeccompConfig`](crate::SeccompConfig)
//! that carries a [`FilePolicy`](crate::config::FilePolicy) routes the
//! path-taking syscalls listed in [`FILE_ACCESS_SYSCALLS`] through
//! `SCMP_ACT_TRACE`. The kernel then stops the process on every such syscall
//! and hands control to this module, which reads the requested path out of the
//! stopped process and applies the policy to it.
//!
//! This module describes the traced syscalls and derives the access each of
//! them requests.

use crate::config::AccessMode;

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

#[cfg(test)]
mod tests {
    use super::*;

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
