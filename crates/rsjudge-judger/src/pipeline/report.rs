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

    /// The output did not match the answer.
    WrongAnswer,

    /// The output matched the answer but not its presentation.
    PresentationError,
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
            Self::WrongAnswer => "wrong answer",
            Self::PresentationError => "presentation error",
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

/// The result of one submission against a whole problem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProblemOutcome {
    /// The verdict of the submission.
    pub verdict: Verdict,

    /// What the compile or check step produced, when the language has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compile: Option<CompileReport>,

    /// The result of every case, in run order.
    pub cases: Vec<CaseResult>,

    /// The result of every subtask, when the problem uses subtasks.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub subtasks: Vec<SubtaskResult>,

    /// The score the submission earned.
    pub score: u32,

    /// The score a fully accepted submission earns.
    pub total_score: u32,
}

/// The result of one case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseResult {
    /// The one-based position of the case in the run order.
    pub index: usize,

    /// The verdict of the case.
    pub verdict: Verdict,

    /// The score the case earned.
    pub score: u32,

    /// The score the case can earn. It is zero for a case inside a subtask,
    /// whose score belongs to the subtask.
    pub max_score: u32,

    /// What the case run used, when it ran at all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution: Option<ExecutionReport>,
}

/// The result of one subtask.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtaskResult {
    /// The one-based position of the subtask.
    pub index: usize,

    /// The verdict of the subtask: accepted when every case passed, otherwise
    /// the verdict of the first failing case.
    pub verdict: Verdict,

    /// The score the subtask earned.
    pub score: u32,

    /// The score the subtask can earn.
    pub max_score: u32,
}
