// SPDX-License-Identifier: Apache-2.0

//! Seccomp profile configuration.
//!
//! This module defines [`SeccompConfig`], which mirrors the extended seccomp
//! profile format used by container runtimes (e.g. the `containers/common`
//! default profile). In addition to the fields defined by the OCI runtime
//! specification ([`oci_spec::runtime::LinuxSeccomp`]), it supports the
//! `archMap` field as well as `includes`/`excludes` matchers on individual
//! syscall rules (keyed on capabilities and architectures).
//!
//! The serde implementations of the OCI enums
//! ([`oci_spec::runtime::LinuxSeccompAction`], [`oci_spec::runtime::Arch`],
//! [`oci_spec::runtime::LinuxSeccompOperator`], ...) are reused so that
//! `SCMP_ACT_*`, `SCMP_ARCH_*` and `SCMP_CMP_*` strings are parsed out of the
//! box.

use std::collections::HashSet;
use std::io;
use std::path::PathBuf;
use std::ptr::NonNull;

use capctl::{Cap, CapSet, CapState};
use libseccomp::{
    ScmpAction, ScmpArch, ScmpArgCompare, ScmpCompareOp, ScmpFilterContext, ScmpSyscall,
};
use oci_spec::runtime::{Arch, LinuxSeccompArg, LinuxSeccompFilterFlag, LinuxSeccompOperator};
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// An action to take on a seccomp filter rule match.
///
/// Modelled after [`ScmpAction`]: actions that carry a return value
/// ([`Errno`](Action::Errno) and [`Trace`](Action::Trace)) hold the value
/// inline instead of in a separate top-level field. The enum is internally
/// tagged by the `action` key so it can be flattened into a [`Syscall`] rule
/// or used directly as the profile's `defaultAction`:
///
/// ```toml
/// [defaultAction]
/// action = "SCMP_ACT_ERRNO"
/// errnoRet = 38
///
/// [[syscalls]]
/// names = ["read"]
/// action = "SCMP_ACT_ALLOW"
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all_fields = "camelCase")]
pub enum Action {
    /// Kill the thread.
    #[serde(rename = "SCMP_ACT_KILL")]
    Kill,
    /// Kill the thread (same as [`Kill`](Action::Kill)).
    #[serde(rename = "SCMP_ACT_KILL_THREAD")]
    KillThread,
    /// Kill the process.
    #[serde(rename = "SCMP_ACT_KILL_PROCESS")]
    KillProcess,
    /// Throw a `SIGSYS` signal.
    #[serde(rename = "SCMP_ACT_TRAP")]
    Trap,
    /// Return the specified error code (`errno_ret` defaults to 0).
    #[serde(rename = "SCMP_ACT_ERRNO")]
    Errno {
        /// The error code to return to the caller.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        errno_ret: Option<u32>,
    },
    /// Notify a userspace process via seccomp notify.
    #[serde(rename = "SCMP_ACT_NOTIFY")]
    Notify,
    /// Notify a tracing process with the specified value (defaults to 0).
    #[serde(rename = "SCMP_ACT_TRACE")]
    Trace {
        /// The value passed to the tracer.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        errno_ret: Option<u16>,
    },
    /// Allow the syscall, logging it first.
    #[serde(rename = "SCMP_ACT_LOG")]
    Log,
    /// Allow the syscall.
    #[serde(rename = "SCMP_ACT_ALLOW")]
    Allow,
}

impl Action {
    /// Convert to the equivalent libseccomp action.
    fn to_scmp(self) -> ScmpAction {
        match self {
            Self::Kill | Self::KillThread => ScmpAction::KillThread,
            Self::KillProcess => ScmpAction::KillProcess,
            Self::Trap => ScmpAction::Trap,
            Self::Errno { errno_ret } => ScmpAction::Errno(errno_ret.unwrap_or(0) as i32),
            Self::Notify => ScmpAction::Notify,
            Self::Trace { errno_ret } => ScmpAction::Trace(errno_ret.unwrap_or(0)),
            Self::Log => ScmpAction::Log,
            Self::Allow => ScmpAction::Allow,
        }
    }
}

/// A seccomp profile.
///
/// This is a superset of [`oci_spec::runtime::LinuxSeccomp`]: it adds the
/// `arch_map` field and richer syscall matchers (`includes`/`excludes`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeccompConfig {
    /// The default action taken for syscalls that match no rule.
    pub default_action: Action,

    /// The architectures the filter applies to.
    ///
    /// When `None`, only the native architecture (plus its sub-architectures
    /// declared via [`arch_map`](Self::arch_map)) is used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub architectures: Option<Vec<Arch>>,

    /// Flags added to the seccomp restriction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flags: Option<Vec<LinuxSeccompFilterFlag>>,

    /// The unix domain socket path used for `SCMP_ACT_NOTIFY`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listener_path: Option<PathBuf>,

    /// Opaque data passed to the seccomp agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listener_metadata: Option<String>,

    /// Maps a "native" architecture to its compatible sub-architectures.
    ///
    /// For each entry whose `architecture` is part of the active architecture
    /// set, its `sub_architectures` are also added to the filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch_map: Option<Vec<ArchMap>>,

    /// The syscall rules.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub syscalls: Option<Vec<Syscall>>,
}

/// Maps an architecture to its compatible sub-architectures.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchMap {
    /// The "native" architecture.
    pub architecture: Arch,

    /// Compatible sub-architectures (e.g. 32-bit compat ABIs).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sub_architectures: Vec<Arch>,
}

/// A syscall rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Syscall {
    /// Names of the syscalls this rule applies to.
    pub names: Vec<String>,

    /// The action taken when the rule matches, flattened so that the rule's
    /// `action` / `errnoRet` keys are parsed as an [`Action`].
    #[serde(flatten)]
    pub action: Action,

    /// Argument comparators for the syscalls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<LinuxSeccompArg>>,

    /// The rule only applies when all of these matchers are satisfied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub includes: Option<Matcher>,

    /// The rule is skipped when any of these matchers is satisfied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excludes: Option<Matcher>,
}

/// A capability / architecture matcher used by [`includes`](Syscall::includes)
/// and [`excludes`](Syscall::excludes).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Matcher {
    /// Capability names (e.g. `CAP_SYS_ADMIN`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caps: Option<Vec<String>>,

    /// Runtime architecture names (e.g. `amd64`, `arm64`, `ppc64le`).
    ///
    /// These use the Go-style architecture names rather than `SCMP_ARCH_*`
    /// constants, matching the `containers/common` profile convention.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arches: Option<Vec<String>>,
}

impl SeccompConfig {
    /// Build a [`ScmpFilterContext`] from this profile.
    ///
    /// The current thread's effective capability set is used to evaluate the
    /// `includes`/`excludes` matchers; call this from the context where the
    /// filter will be loaded (typically the unprivileged child process).
    pub fn build_filter(&self) -> Result<ScmpFilterContext> {
        let state = CapState::get_current().map_err(io::Error::other)?;
        self.build_filter_with_caps(&state.effective)
    }

    /// Build a [`ScmpFilterContext`] using the provided effective capability
    /// set to evaluate `includes`/`excludes`.
    pub fn build_filter_with_caps(&self, effective_caps: &CapSet) -> Result<ScmpFilterContext> {
        let mut ctx =
            ScmpFilterContext::new(self.default_action.to_scmp()).map_err(io::Error::other)?;

        let native = ScmpArch::native();
        let mut arches: Vec<ScmpArch> = match &self.architectures {
            Some(list) => list.iter().map(|a| arch_to_scmp(*a)).collect(),
            None => vec![native],
        };

        // Expand sub-architectures declared via `arch_map`.
        let mut present: HashSet<ScmpArch> = arches.iter().copied().collect();
        if let Some(maps) = &self.arch_map {
            for map in maps {
                let main = arch_to_scmp(map.architecture);
                if present.contains(&main) {
                    for sub in &map.sub_architectures {
                        let s = arch_to_scmp(*sub);
                        if present.insert(s) {
                            arches.push(s);
                        }
                    }
                }
            }
        }

        for arch in arches {
            ctx.add_arch(arch).map_err(io::Error::other)?;
        }

        if let Some(flags) = &self.flags {
            for flag in flags {
                apply_filter_flag(&mut ctx, *flag)?;
            }
        }

        if let Some(syscalls) = &self.syscalls {
            for rule in syscalls {
                if !rule.matches(effective_caps, native) {
                    continue;
                }

                let action = rule.action.to_scmp();
                let comparators: Vec<ScmpArgCompare> = rule
                    .args
                    .as_ref()
                    .map(|args| args.iter().map(arg_to_scmp).collect())
                    .unwrap_or_default();

                for name in &rule.names {
                    let syscall = ScmpSyscall::from_name(name).map_err(io::Error::other)?;
                    if comparators.is_empty() {
                        ctx.add_rule(action, syscall).map_err(io::Error::other)?;
                    } else {
                        ctx.add_rule_conditional(action, syscall, &comparators)
                            .map_err(io::Error::other)?;
                    }
                }
            }
        }

        Ok(ctx)
    }
}

/// A compiled seccomp BPF program, ready to be loaded into a process.
///
/// The bytecode is generated by libseccomp in the parent process, which may
/// allocate freely: it is exported to an anonymous in-memory file with
/// [`ScmpFilterContext::export_bpf`] (available since libseccomp 2.2) and
/// then mapped into the address space as a read-only, private mapping.
/// `fork` lets the child inherit that mapping at the same address, so the
/// `pre_exec` hook installs the program with two async-signal-safe `prctl`
/// calls and performs no allocation of its own. `execve` tears down the
/// child's copy of the mapping, while [`Drop`] unmaps the parent's copy.
#[derive(Debug)]
pub struct SeccompFilter {
    /// Start of the read-only private mapping holding the BPF bytecode.
    mapping: NonNull<core::ffi::c_void>,

    /// Mapping length in bytes (always a multiple of `sock_filter`).
    len: usize,
}

// SAFETY: the mapping is `PROT_READ` with `MAP_PRIVATE`, so the bytes are
// immutable through this pointer and cross-thread access only reads them;
// dereferencing happens solely in the forked child's `pre_exec` hook.
unsafe impl Send for SeccompFilter {}
unsafe impl Sync for SeccompFilter {}

impl SeccompFilter {
    /// Build a [`SeccompFilter`] from a [`SeccompConfig`], evaluating the
    /// `includes`/`excludes` matchers against the given effective capability
    /// set.
    ///
    /// The capability set should reflect the capabilities the child process
    /// will have when the filter is loaded (typically empty for an
    /// unprivileged submission process).
    pub fn from_config(config: &SeccompConfig, effective_caps: &CapSet) -> Result<Self> {
        use std::num::NonZeroUsize;
        use std::os::fd::AsFd;

        use nix::sys::memfd::{MFdFlags, memfd_create};
        use nix::sys::mman::{MapFlags, ProtFlags, mmap};
        use nix::unistd::{Whence, lseek};

        let ctx = config.build_filter_with_caps(effective_caps)?;

        let memfd =
            memfd_create(c"rsjudge-seccomp", MFdFlags::MFD_CLOEXEC).map_err(io::Error::other)?;
        ctx.export_bpf(memfd.as_fd()).map_err(io::Error::other)?;

        // The export size is the memfd's file size.
        let len = lseek(memfd.as_fd(), 0, Whence::SeekEnd).map_err(io::Error::other)? as usize;
        let len = NonZeroUsize::new(len)
            .ok_or_else(|| io::Error::other("libseccomp exported an empty BPF program"))?;

        // SAFETY: `memfd` contains the bytecode just exported; the mapping is
        // read-only and private.
        let mapping = unsafe {
            mmap(
                None,
                len,
                ProtFlags::PROT_READ,
                MapFlags::MAP_PRIVATE,
                memfd.as_fd(),
                0,
            )
        }
        .map_err(io::Error::other)?;

        // The mapping holds its own reference to the file, so the fd closes.
        drop(memfd);

        Ok(Self {
            mapping,
            len: len.get(),
        })
    }

    /// Load the filter into the calling process.
    ///
    /// This sets `PR_SET_NO_NEW_PRIVS` (via
    /// [`nix::sys::prctl::set_no_new_privs`]) and then installs the BPF
    /// program via `PR_SET_SECCOMP`. It is intended to be called from a
    /// `pre_exec` hook in the forked child process.
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` if either `prctl` call fails.
    pub fn load(&self) -> io::Result<()> {
        use std::ffi::c_ushort;

        use nix::libc::{PR_SET_SECCOMP, SECCOMP_MODE_FILTER, prctl, sock_filter, sock_fprog};

        // PR_SET_NO_NEW_PRIVS is required to load a seccomp filter without
        // CAP_SYS_ADMIN.
        nix::sys::prctl::set_no_new_privs().map_err(io::Error::from)?;

        debug_assert_eq!(
            self.len % std::mem::size_of::<sock_filter>(),
            0,
            "BPF bytecode length must be a multiple of sock_filter",
        );
        let mut fprog = sock_fprog {
            len: (self.len / std::mem::size_of::<sock_filter>()) as c_ushort,
            filter: self.mapping.as_ptr() as *mut sock_filter,
        };

        if unsafe { prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, &mut fprog) } != 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(())
    }
}

impl Drop for SeccompFilter {
    fn drop(&mut self) {
        // SAFETY: `mapping`/`len` describe exactly the mapping created in
        // `from_config`, used until the filter is dropped. The child's copy
        // is released by `execve`, so this only ever runs in the parent.
        unsafe {
            let _ = nix::sys::mman::munmap(self.mapping, self.len);
        }
    }
}

impl Syscall {
    /// Determine whether this rule should be applied, given the effective
    /// capability set and native architecture.
    fn matches(&self, caps: &CapSet, native: ScmpArch) -> bool {
        if let Some(inc) = &self.includes
            && !inc.matches_all(caps, native)
        {
            return false;
        }
        if let Some(exc) = &self.excludes
            && exc.matches_any(caps, native)
        {
            return false;
        }
        true
    }
}

impl Matcher {
    /// Returns `true` if **all** constraints of the matcher are satisfied.
    fn matches_all(&self, caps: &CapSet, native: ScmpArch) -> bool {
        if let Some(required) = &self.caps {
            for name in required {
                match name.parse::<Cap>() {
                    Ok(cap) if caps.has(cap) => {}
                    _ => return false,
                }
            }
        }
        if let Some(arches) = &self.arches
            && !arches.iter().any(|a| go_arch_to_scmp(a) == Some(native))
        {
            return false;
        }
        true
    }

    /// Returns `true` if **any** constraint of the matcher is satisfied.
    fn matches_any(&self, caps: &CapSet, native: ScmpArch) -> bool {
        if let Some(forbidden) = &self.caps {
            for name in forbidden {
                if let Ok(cap) = name.parse::<Cap>()
                    && caps.has(cap)
                {
                    return true;
                }
            }
        }
        if let Some(arches) = &self.arches
            && arches.iter().any(|a| go_arch_to_scmp(a) == Some(native))
        {
            return true;
        }
        false
    }
}

/// Convert an OCI architecture token into a libseccomp architecture.
fn arch_to_scmp(arch: Arch) -> ScmpArch {
    match arch {
        Arch::ScmpArchNative => ScmpArch::Native,
        Arch::ScmpArchX86 => ScmpArch::X86,
        Arch::ScmpArchX86_64 => ScmpArch::X8664,
        Arch::ScmpArchX32 => ScmpArch::X32,
        Arch::ScmpArchArm => ScmpArch::Arm,
        Arch::ScmpArchAarch64 => ScmpArch::Aarch64,
        Arch::ScmpArchMips => ScmpArch::Mips,
        Arch::ScmpArchMips64 => ScmpArch::Mips64,
        Arch::ScmpArchMips64n32 => ScmpArch::Mips64N32,
        Arch::ScmpArchMipsel => ScmpArch::Mipsel,
        Arch::ScmpArchMipsel64 => ScmpArch::Mipsel64,
        Arch::ScmpArchMipsel64n32 => ScmpArch::Mipsel64N32,
        Arch::ScmpArchPpc => ScmpArch::Ppc,
        Arch::ScmpArchPpc64 => ScmpArch::Ppc64,
        Arch::ScmpArchPpc64le => ScmpArch::Ppc64Le,
        Arch::ScmpArchS390 => ScmpArch::S390,
        Arch::ScmpArchS390x => ScmpArch::S390X,
        Arch::ScmpArchParisc => ScmpArch::Parisc,
        Arch::ScmpArchParisc64 => ScmpArch::Parisc64,
        Arch::ScmpArchRiscv64 => ScmpArch::Riscv64,
        Arch::ScmpArchLoongarch64 => ScmpArch::Loongarch64,
        Arch::ScmpArchM68k => ScmpArch::M68k,
        Arch::ScmpArchSh => ScmpArch::Sh,
        Arch::ScmpArchSheb => ScmpArch::Sheb,
    }
}

/// Convert an OCI seccomp argument comparator into a libseccomp comparator.
fn arg_to_scmp(arg: &LinuxSeccompArg) -> ScmpArgCompare {
    let op = match arg.op() {
        LinuxSeccompOperator::ScmpCmpNe => ScmpCompareOp::NotEqual,
        LinuxSeccompOperator::ScmpCmpLt => ScmpCompareOp::Less,
        LinuxSeccompOperator::ScmpCmpLe => ScmpCompareOp::LessOrEqual,
        LinuxSeccompOperator::ScmpCmpEq => ScmpCompareOp::Equal,
        LinuxSeccompOperator::ScmpCmpGe => ScmpCompareOp::GreaterEqual,
        LinuxSeccompOperator::ScmpCmpGt => ScmpCompareOp::Greater,
        LinuxSeccompOperator::ScmpCmpMaskedEq => ScmpCompareOp::MaskedEqual(arg.value()),
    };

    // For `SCMP_CMP_MASKED_EQ`, `value` is the mask and `valueTwo` is the
    // datum to compare against; for every other operator `value` is the datum.
    let datum = if matches!(arg.op(), LinuxSeccompOperator::ScmpCmpMaskedEq) {
        arg.value_two().unwrap_or(0)
    } else {
        arg.value()
    };

    ScmpArgCompare::new(arg.index() as u32, op, datum)
}

/// Convert a Go-style runtime architecture name into a libseccomp architecture.
///
/// These names are used by the `containers/common` profile in the
/// `includes.arches` / `excludes.arches` fields.
fn go_arch_to_scmp(arch: &str) -> Option<ScmpArch> {
    Some(match arch {
        "amd64" => ScmpArch::X8664,
        "arm64" => ScmpArch::Aarch64,
        "arm" => ScmpArch::Arm,
        "x86" => ScmpArch::X86,
        "x32" => ScmpArch::X32,
        "ppc64le" => ScmpArch::Ppc64Le,
        "ppc64" => ScmpArch::Ppc64,
        "ppc" => ScmpArch::Ppc,
        "s390x" => ScmpArch::S390X,
        "s390" => ScmpArch::S390,
        "riscv64" => ScmpArch::Riscv64,
        "mips64" => ScmpArch::Mips64,
        "mips64n32" => ScmpArch::Mips64N32,
        "mips" => ScmpArch::Mips,
        "mipsel" => ScmpArch::Mipsel,
        "mipsel64" => ScmpArch::Mipsel64,
        "mipsel64n32" => ScmpArch::Mipsel64N32,
        "loongarch64" => ScmpArch::Loongarch64,
        "m68k" => ScmpArch::M68k,
        "sh" => ScmpArch::Sh,
        "sheb" => ScmpArch::Sheb,
        "parisc" => ScmpArch::Parisc,
        "parisc64" => ScmpArch::Parisc64,
        _ => return None,
    })
}

/// Apply a seccomp filter flag to the filter context.
fn apply_filter_flag(ctx: &mut ScmpFilterContext, flag: LinuxSeccompFilterFlag) -> Result<()> {
    match flag {
        LinuxSeccompFilterFlag::SeccompFilterFlagLog => {
            ctx.set_ctl_log(true).map_err(io::Error::other)?;
        }
        LinuxSeccompFilterFlag::SeccompFilterFlagTsync => {
            ctx.set_ctl_tsync(true).map_err(io::Error::other)?;
        }
        LinuxSeccompFilterFlag::SeccompFilterFlagSpecAllow => {
            ctx.set_ctl_ssb(false).map_err(io::Error::other)?;
        }
        LinuxSeccompFilterFlag::SeccompFilterFlagWaitKillableRecv => {
            ctx.set_ctl_waitkill(true).map_err(io::Error::other)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn parse_default_action_and_errno() {
        let config: SeccompConfig = serde_json::from_value(json!({
            "defaultAction": { "action": "SCMP_ACT_ERRNO", "errnoRet": 38 },
        }))
        .expect("failed to parse");

        assert_eq!(
            config.default_action,
            Action::Errno {
                errno_ret: Some(38)
            }
        );
        assert!(config.architectures.is_none());
        assert!(config.syscalls.is_none());
    }

    #[test]
    fn parse_arch_map() {
        let config: SeccompConfig = serde_json::from_value(json!({
            "defaultAction": { "action": "SCMP_ACT_ALLOW" },
            "archMap": [
                {
                    "architecture": "SCMP_ARCH_X86_64",
                    "subArchitectures": ["SCMP_ARCH_X86", "SCMP_ARCH_X32"],
                },
                {
                    "architecture": "SCMP_ARCH_AARCH64",
                    "subArchitectures": ["SCMP_ARCH_ARM"],
                },
            ],
        }))
        .expect("failed to parse");

        let maps = config.arch_map.as_ref().unwrap();
        assert_eq!(maps.len(), 2);
        assert_eq!(maps[0].architecture, Arch::ScmpArchX86_64);
        assert_eq!(
            maps[0].sub_architectures,
            vec![Arch::ScmpArchX86, Arch::ScmpArchX32]
        );
    }

    #[test]
    fn parse_syscall_with_args() {
        let config: SeccompConfig = serde_json::from_value(json!({
            "defaultAction": { "action": "SCMP_ACT_ALLOW" },
            "syscalls": [
                {
                    "names": ["personality"],
                    "action": "SCMP_ACT_ALLOW",
                    "args": [
                        { "index": 0, "value": 0, "op": "SCMP_CMP_EQ" },
                    ],
                },
            ],
        }))
        .expect("failed to parse");

        let syscalls = config.syscalls.as_ref().unwrap();
        assert_eq!(syscalls.len(), 1);

        let rule = &syscalls[0];
        assert_eq!(rule.names, vec!["personality".to_string()]);
        assert_eq!(rule.action, Action::Allow);

        let args = rule.args.as_ref().unwrap();
        assert_eq!(args.len(), 1);
        assert_eq!(args[0].index(), 0);
        assert_eq!(args[0].value(), 0);
        assert_eq!(args[0].op(), LinuxSeccompOperator::ScmpCmpEq);
    }

    #[test]
    fn parse_includes_excludes_matchers() {
        let config: SeccompConfig = serde_json::from_value(json!({
            "defaultAction": { "action": "SCMP_ACT_ERRNO" },
            "syscalls": [
                {
                    "names": ["bpf"],
                    "action": "SCMP_ACT_ALLOW",
                    "includes": { "caps": ["CAP_BPF"] },
                },
                {
                    "names": ["bpf"],
                    "action": "SCMP_ACT_ERRNO",
                    "errnoRet": 1,
                    "excludes": { "caps": ["CAP_SYS_ADMIN", "CAP_BPF"] },
                },
                {
                    "names": ["arch_prctl"],
                    "action": "SCMP_ACT_ALLOW",
                    "includes": { "arches": ["amd64", "x32"] },
                },
            ],
        }))
        .expect("failed to parse");

        let syscalls = config.syscalls.as_ref().unwrap();

        let bpf_inc = syscalls
            .iter()
            .find(|s| s.names == vec!["bpf".to_string()] && s.includes.is_some())
            .unwrap();
        assert_eq!(
            bpf_inc.includes.as_ref().unwrap().caps.as_ref().unwrap(),
            &["CAP_BPF".to_string()]
        );

        let bpf_exc = syscalls
            .iter()
            .find(|s| s.names == vec!["bpf".to_string()] && s.excludes.is_some())
            .unwrap();
        assert_eq!(
            bpf_exc.excludes.as_ref().unwrap().caps.as_ref().unwrap(),
            &["CAP_SYS_ADMIN".to_string(), "CAP_BPF".to_string()]
        );

        let arch_prctl = syscalls
            .iter()
            .find(|s| s.names.contains(&"arch_prctl".to_string()))
            .unwrap();
        assert_eq!(
            arch_prctl
                .includes
                .as_ref()
                .unwrap()
                .arches
                .as_ref()
                .unwrap(),
            &["amd64".to_string(), "x32".to_string()]
        );
    }

    #[test]
    fn build_filter_minimal() {
        let config: SeccompConfig = serde_json::from_value(json!({
            "defaultAction": { "action": "SCMP_ACT_ERRNO", "errnoRet": 38 },
            "syscalls": [
                {
                    "names": ["read", "write"],
                    "action": "SCMP_ACT_ALLOW",
                },
                {
                    "names": ["swapon"],
                    "action": "SCMP_ACT_ERRNO",
                    "errnoRet": 1,
                },
            ],
        }))
        .expect("failed to parse");

        let empty = CapSet::empty();
        let ctx = config
            .build_filter_with_caps(&empty)
            .expect("failed to build filter");

        // Build the filter without loading it, so the test process remains
        // unaffected.
        drop(ctx);
    }
}
