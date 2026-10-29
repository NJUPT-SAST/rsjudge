// SPDX-License-Identifier: Apache-2.0

//! The result types a pipeline run produces.

use serde::{Deserialize, Serialize};

/// The verdict of one submission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    /// The program compiled and exited with status 0.
    Accepted,

    /// The program did not compile.
    CompileError,

    /// The program exited non-zero or was killed by a signal.
    RuntimeError,

    /// The program exceeded its time limit.
    TimeLimitExceeded,

    /// The program reached its memory limit.
    MemoryLimitExceeded,
}

impl Verdict {
    /// The verdict as a short human-readable label.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::CompileError => "compile error",
            Self::RuntimeError => "runtime error",
            Self::TimeLimitExceeded => "time limit exceeded",
            Self::MemoryLimitExceeded => "memory limit exceeded",
        }
    }
}

/// What the supervisor did with the calls of one syscall.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyscallCount {
    /// The syscall name.
    pub name: String,

    /// The number of calls the supervisor served.
    pub count: u64,

    /// The number of calls the file policy refused with `ENOSYS`.
    pub denied: u64,

    /// The number of calls that ended the submission's process group.
    pub killed: u64,
}

/// What the compile step of a language produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileReport {
    /// Whether the compiler exited with status 0.
    pub success: bool,

    /// How long the compiler ran.
    pub duration_ms: u64,

    /// What the compiler wrote to standard output.
    pub stdout: String,

    /// What the compiler wrote to standard error.
    pub stderr: String,
}

/// What a single supervised run used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionReport {
    /// The exit code of the program, when it exited normally.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,

    /// The signal that ended the program, when one did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal: Option<i32>,

    /// Whether the supervisor stopped the program at the elapsed-time limit.
    pub timed_out: bool,

    /// Total elapsed time of the run.
    pub elapsed_ms: u64,

    /// CPU time the program used, as `wait4` reported it.
    pub cpu_time_ms: u64,

    /// Peak resident set size of the program, in kibibytes.
    pub max_rss_kb: u64,

    /// The traced syscalls, in name order.
    pub syscalls: Vec<SyscallCount>,

    /// What the program wrote to standard output.
    pub stdout: String,

    /// What the program wrote to standard error.
    pub stderr: String,
}

impl ExecutionReport {
    /// The number of traced file-access calls the policy refused.
    #[must_use]
    pub fn denied_calls(&self) -> u64 {
        self.syscalls.iter().map(|syscall| syscall.denied).sum()
    }

    /// The number of traced file-access calls that ended the process group.
    #[must_use]
    pub fn killed_calls(&self) -> u64 {
        self.syscalls.iter().map(|syscall| syscall.killed).sum()
    }
}

/// The result of one submission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseOutcome {
    /// The verdict of the submission.
    pub verdict: Verdict,

    /// What the compiler produced, when the language has a compile step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compile: Option<CompileReport>,

    /// What the run used, when it ran at all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution: Option<ExecutionReport>,
}
