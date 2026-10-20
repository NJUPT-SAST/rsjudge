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
    /// Whether this action restricts a syscall more tightly than the
    /// file-access policy does, so the policy's trace rule yields to it.
    ///
    /// `Log` and `Notify` still permit the call, so a traced syscall keeps its
    /// trace rule and stays under the policy.
    fn outranks_policy(self) -> bool {
        matches!(
            self,
            Self::Kill | Self::KillThread | Self::KillProcess | Self::Trap | Self::Errno { .. }
        )
    }

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

    /// The file-access policy enforced by the ptrace supervisor.
    ///
    /// When present, the filter generated by
    /// [`build_filter_with_caps`](SeccompConfig::build_filter_with_caps)
    /// routes the path-taking syscalls the supervisor intercepts through
    /// `SCMP_ACT_TRACE`, and the supervisor resolves each requested path and
    /// applies this policy to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_policy: Option<FilePolicy>,
}

/// The access a file operation requests, or a rule permits.
///
/// [`Any`](AccessMode::Any) covers an operation that reads and writes, so a
/// request for both is only permitted by a rule that grants both.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AccessMode {
    /// Reading only.
    Read,
    /// Writing only.
    Write,
    /// Reading and writing.
    #[default]
    Any,
}

impl AccessMode {
    /// Returns `true` when this mode covers the requested access.
    #[must_use]
    pub fn covers(self, requested: Self) -> bool {
        matches!(self, Self::Any) || self == requested
    }
}

/// How the paths of a [`PathRule`] are matched against a requested path.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PathMatch {
    /// The requested path equals the rule's path.
    #[default]
    Exact,
    /// The rule's path is a leading path of the requested path, compared
    /// component by component: `/etc` is a prefix of `/etc/passwd`, while
    /// `/etc` and `/etcetera` are distinct.
    Prefix,
}

impl PathMatch {
    /// Returns `true` when the rule's path matches the requested path.
    #[must_use]
    fn matches(self, rule: &std::path::Path, requested: &std::path::Path) -> bool {
        match self {
            Self::Exact => rule == requested,
            Self::Prefix => requested.starts_with(rule),
        }
    }
}

/// The action taken when a [`PathRule`] matches.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileAction {
    /// Let the file operation proceed.
    Allow,
    /// Fail the file operation.
    #[default]
    Deny,
    /// Kill the process performing the file operation.
    Kill,
}

/// A file-access rule.
///
/// The rule matches when the requested path is covered by
/// [`paths`](Self::paths) under [`match`](Self::r#match) and the requested
/// access is covered by [`access`](Self::access).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathRule {
    /// The paths this rule applies to.
    pub paths: Vec<PathBuf>,

    /// The matching mode used for [`paths`](Self::paths).
    #[serde(rename = "match", default)]
    pub r#match: PathMatch,

    /// The access this rule covers.
    #[serde(default)]
    pub access: AccessMode,

    /// The action taken when this rule matches.
    pub action: FileAction,
}

/// The file-access policy applied to the traced syscalls.
///
/// The first rule that matches a requested path wins; a request covered by no
/// rule receives [`default_action`](Self::default_action), which keeps the
/// policy closed by default.
///
/// ```toml
/// [filePolicy]
/// defaultAction = "deny"
///
/// [[filePolicy.rules]]
/// paths = ["/usr", "/lib"]
/// match = "prefix"
/// access = "read"
/// action = "allow"
///
/// [[filePolicy.rules]]
/// paths = ["/etc/shadow"]
/// action = "kill"
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePolicy {
    /// The action taken for requests no rule matches.
    #[serde(default)]
    pub default_action: FileAction,

    /// The rules, evaluated in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<PathRule>,
}

impl FilePolicy {
    /// Decide the action for a path requested with the given access.
    #[must_use]
    pub fn evaluate(&self, path: &std::path::Path, requested: AccessMode) -> FileAction {
        self.rules
            .iter()
            .find(|rule| {
                rule.access.covers(requested)
                    && rule.paths.iter().any(|p| rule.r#match.matches(p, path))
            })
            .map_or(self.default_action, |rule| rule.action)
    }
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

        // A filter rule applies to every architecture it declares, while the
        // supervisor reads registers in the terms of the native architecture
        // only: an `openat` of a compatible 32-bit ABI carries another number
        // and another argument layout, which the supervisor would misread.
        if self.file_policy.is_some()
            && arches
                .iter()
                .any(|arch| *arch != native && *arch != ScmpArch::Native)
        {
            return Err(io::Error::other(
                "a file-access policy requires a filter built for the native architecture only",
            )
            .into());
        }

        for arch in arches {
            ctx.add_arch(arch).map_err(io::Error::other)?;
        }

        if let Some(flags) = &self.flags {
            for flag in flags {
                apply_filter_flag(&mut ctx, *flag)?;
            }
        }

        // The traced syscalls and the profile rules that outrank the policy.
        let traced: HashSet<&'static str> = match &self.file_policy {
            Some(_) => crate::tracer::file_access_syscall_names().collect(),
            None => HashSet::new(),
        };
        let mut restricted: HashSet<&'static str> = HashSet::new();

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

                for name in Self::rule_names(rule, &traced, &mut restricted) {
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

        if self.file_policy.is_some() {
            for syscall in crate::tracer::FILE_ACCESS_SYSCALLS {
                // A profile rule that restricts this syscall more tightly than
                // the policy would keeps its own action.
                if restricted.contains(syscall.name) {
                    continue;
                }

                // A syscall missing from the running architecture drops out.
                let Ok(scmp) = ScmpSyscall::from_name(syscall.name) else {
                    continue;
                };
                ctx.add_rule(ScmpAction::Trace(0), scmp)
                    .map_err(io::Error::other)?;
            }
        }

        Ok(ctx)
    }

    /// The names of `rule` that reach the filter.
    ///
    /// With a file-access policy in place the policy decides the fate of the
    /// traced syscalls, so a plain `Allow` rule for such a name steps aside to
    /// let the `SCMP_ACT_TRACE` rule take effect. A rule that restricts the
    /// call keeps its own action and records the name in `restricted` so that
    /// the trace rule yields to it, since the policy cannot restate the
    /// profile's restriction. A rule that narrows the call to particular
    /// arguments keeps its place as well, and the trace rule follows it: the
    /// unconditional trace rule covers the whole syscall, so the filter traces
    /// every call of it and the policy judges them.
    fn rule_names<'a>(
        rule: &'a Syscall,
        traced: &HashSet<&'static str>,
        restricted: &mut HashSet<&'static str>,
    ) -> Vec<&'a str> {
        let conditional = rule.args.as_ref().is_some_and(|args| !args.is_empty());

        rule.names
            .iter()
            .filter(|name| {
                let Some(name) = traced.get(name.as_str()) else {
                    return true;
                };

                if rule.action.outranks_policy() {
                    restricted.insert(name);
                    return true;
                }

                // A conditional rule keeps its place, and the trace rule
                // follows it, so every call of the syscall reaches the policy.
                conditional
            })
            .map(String::as_str)
            .collect()
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
    fn parse_file_policy() {
        let config: SeccompConfig = serde_json::from_value(json!({
            "defaultAction": { "action": "SCMP_ACT_ALLOW" },
            "filePolicy": {
                "defaultAction": "deny",
                "rules": [
                    {
                        "paths": ["/usr", "/lib"],
                        "match": "prefix",
                        "access": "read",
                        "action": "allow",
                    },
                    {
                        "paths": ["/etc/shadow"],
                        "action": "kill",
                    },
                ],
            },
        }))
        .expect("failed to parse");

        let policy = config.file_policy.as_ref().expect("missing file policy");
        assert_eq!(policy.default_action, FileAction::Deny);
        assert_eq!(policy.rules.len(), 2);

        assert_eq!(policy.rules[0].r#match, PathMatch::Prefix);
        assert_eq!(policy.rules[0].access, AccessMode::Read);
        assert_eq!(policy.rules[0].action, FileAction::Allow);

        // The `match` and `access` keys default to exact matching over any
        // access.
        assert_eq!(policy.rules[1].r#match, PathMatch::Exact);
        assert_eq!(policy.rules[1].access, AccessMode::Any);
        assert_eq!(policy.rules[1].action, FileAction::Kill);
    }

    #[test]
    fn file_policy_matching() {
        let policy: FilePolicy = serde_json::from_value(json!({
            "defaultAction": "deny",
            "rules": [
                {
                    "paths": ["/usr", "/lib"],
                    "match": "prefix",
                    "access": "read",
                    "action": "allow",
                },
                {
                    "paths": ["/sandbox/output"],
                    "match": "prefix",
                    "access": "write",
                    "action": "allow",
                },
                {
                    "paths": ["/etc/shadow"],
                    "action": "kill",
                },
            ],
        }))
        .expect("failed to parse");

        use std::path::Path;

        // A prefix rule covers nested paths only.
        assert_eq!(
            policy.evaluate(Path::new("/usr/lib/libc.so"), AccessMode::Read),
            FileAction::Allow
        );
        assert_eq!(
            policy.evaluate(Path::new("/usrlocal/x"), AccessMode::Read),
            FileAction::Deny
        );

        // A read rule covers read requests, while write requests fall through.
        assert_eq!(
            policy.evaluate(Path::new("/usr/bin/cat"), AccessMode::Write),
            FileAction::Deny
        );
        assert_eq!(
            policy.evaluate(Path::new("/sandbox/output/a.out"), AccessMode::Write),
            FileAction::Allow
        );
        assert_eq!(
            policy.evaluate(Path::new("/sandbox/output/a.out"), AccessMode::Read),
            FileAction::Deny
        );

        // The first matching rule wins.
        assert_eq!(
            policy.evaluate(Path::new("/etc/shadow"), AccessMode::Read),
            FileAction::Kill
        );

        // An exact rule stays confined to its own path.
        assert_eq!(
            policy.evaluate(Path::new("/etc/shadow/backup"), AccessMode::Read),
            FileAction::Deny
        );
    }

    #[test]
    fn access_mode_covers() {
        assert!(AccessMode::Any.covers(AccessMode::Read));
        assert!(AccessMode::Any.covers(AccessMode::Write));
        assert!(AccessMode::Any.covers(AccessMode::Any));
        assert!(AccessMode::Read.covers(AccessMode::Read));
        assert!(!AccessMode::Read.covers(AccessMode::Write));
        assert!(!AccessMode::Write.covers(AccessMode::Read));
        assert!(!AccessMode::Write.covers(AccessMode::Any));
        assert!(!AccessMode::Read.covers(AccessMode::Any));
    }

    #[test]
    fn build_filter_with_file_policy() {
        // The profile allows `openat` and `read`; with a file-access policy in
        // place the `allow` rule for `openat` steps aside so the traced
        // syscall reaches the supervisor, while `read` keeps its rule.
        let config: SeccompConfig = serde_json::from_value(json!({
            "defaultAction": { "action": "SCMP_ACT_ERRNO", "errnoRet": 38 },
            "syscalls": [
                { "names": ["openat", "read"], "action": "SCMP_ACT_ALLOW" },
            ],
            "filePolicy": { "defaultAction": "deny" },
        }))
        .expect("failed to parse");

        let ctx = config
            .build_filter_with_caps(&CapSet::empty())
            .expect("failed to build filter");
        drop(ctx);
    }

    #[test]
    fn build_filter_with_file_policy_and_restricting_rule() {
        // The profile fails `openat` outright, which outranks the policy, so
        // the filter keeps the profile's action.
        let config: SeccompConfig = serde_json::from_value(json!({
            "defaultAction": { "action": "SCMP_ACT_ALLOW" },
            "syscalls": [
                { "names": ["openat"], "action": "SCMP_ACT_ERRNO", "errnoRet": 1 },
            ],
            "filePolicy": { "defaultAction": "deny" },
        }))
        .expect("failed to parse");

        let ctx = config
            .build_filter_with_caps(&CapSet::empty())
            .expect("failed to build filter");
        drop(ctx);
    }

    #[test]
    fn build_filter_rejects_sub_architectures_with_a_file_policy() {
        // A filter rule reaches every architecture it declares, while the
        // supervisor reads registers in the terms of the native architecture,
        // so a compatible 32-bit ABI would escape the policy.
        let config: SeccompConfig = serde_json::from_value(json!({
            "defaultAction": { "action": "SCMP_ACT_ALLOW" },
            "architectures": ["SCMP_ARCH_NATIVE", "SCMP_ARCH_X86"],
            "filePolicy": { "defaultAction": "deny" },
        }))
        .expect("failed to parse");

        assert!(
            config.build_filter_with_caps(&CapSet::empty()).is_err(),
            "a policy was built for an ABI the supervisor cannot read"
        );

        // Without a policy the same architectures still build.
        let mut config = config;
        config.file_policy = None;
        config
            .build_filter_with_caps(&CapSet::empty())
            .expect("a profile without a policy lost its architectures");
    }

    #[test]
    fn build_filter_accepts_the_native_architecture_with_a_file_policy() {
        let config: SeccompConfig = serde_json::from_value(json!({
            "defaultAction": { "action": "SCMP_ACT_ALLOW" },
            "architectures": ["SCMP_ARCH_NATIVE"],
            "filePolicy": { "defaultAction": "deny" },
        }))
        .expect("failed to parse");

        config
            .build_filter_with_caps(&CapSet::empty())
            .expect("the native architecture was refused a policy");
    }

    #[test]
    fn a_profile_action_outranks_the_policy_only_when_it_restricts() {
        // These actions refuse the call, so the policy's trace rule yields.
        assert!(Action::Kill.outranks_policy());
        assert!(Action::KillThread.outranks_policy());
        assert!(Action::KillProcess.outranks_policy());
        assert!(Action::Trap.outranks_policy());
        assert!(Action::Errno { errno_ret: Some(1) }.outranks_policy());

        // These actions still let the call through, so the traced syscall
        // stays under the policy.
        assert!(!Action::Allow.outranks_policy());
        assert!(!Action::Log.outranks_policy());
        assert!(!Action::Notify.outranks_policy());
    }

    #[test]
    fn build_filter_keeps_an_argument_constrained_allow_rule() {
        // The profile allows `openat` only with a particular flag word. The
        // conditional rule keeps its place, and the trace rule follows it, so
        // every call of the syscall reaches the supervisor.
        let config: SeccompConfig = serde_json::from_value(json!({
            "defaultAction": { "action": "SCMP_ACT_ERRNO", "errnoRet": 1 },
            "syscalls": [
                {
                    "names": ["openat"],
                    "action": "SCMP_ACT_ALLOW",
                    "args": [
                        { "index": 2, "value": 0, "op": "SCMP_CMP_EQ" },
                    ],
                },
            ],
            "filePolicy": { "defaultAction": "deny" },
        }))
        .expect("failed to parse");

        config
            .build_filter_with_caps(&CapSet::empty())
            .expect("a conditional allow rule was dropped");
    }

    #[test]
    fn an_argument_constrained_allow_rule_still_gets_the_trace_rule() {
        let rule: Syscall = serde_json::from_value(json!({
            "names": ["openat"],
            "action": "SCMP_ACT_ALLOW",
            "args": [
                { "index": 2, "value": 0, "op": "SCMP_CMP_EQ" },
            ],
        }))
        .expect("failed to build the rule");

        let traced: HashSet<&'static str> = crate::tracer::file_access_syscall_names().collect();
        let mut restricted = HashSet::new();
        let names = SeccompConfig::rule_names(&rule, &traced, &mut restricted);

        // The rule reaches the filter, and the trace rule follows it, so every
        // call of the syscall reaches the policy.
        assert_eq!(names, vec!["openat"]);
        assert!(
            restricted.is_empty(),
            "an argument-constrained allow rule disabled the policy trace rule"
        );
    }

    #[test]
    fn an_argument_constrained_refusal_keeps_its_own_action() {
        let rule: Syscall = serde_json::from_value(json!({
            "names": ["openat"],
            "action": "SCMP_ACT_ERRNO",
            "errnoRet": 1,
            "args": [
                { "index": 2, "value": 0, "op": "SCMP_CMP_EQ" },
            ],
        }))
        .expect("failed to build the rule");

        let traced: HashSet<&'static str> = crate::tracer::file_access_syscall_names().collect();
        let mut restricted = HashSet::new();
        let names = SeccompConfig::rule_names(&rule, &traced, &mut restricted);

        // The refusal outranks the policy, so the trace rule yields to it.
        assert_eq!(names, vec!["openat"]);
        assert!(
            restricted.contains("openat"),
            "a conditional refusal let the policy trace rule through"
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
