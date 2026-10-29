// SPDX-License-Identifier: Apache-2.0

//! Errors a pipeline run can fail with.

use std::time::Duration;

/// An error that prevented the pipeline from producing a [`CaseOutcome`].
///
/// A failing submission is not an error: a program that exits non-zero, runs
/// out of time or trips the file policy still produces an outcome that
/// describes what happened. The variants here cover the failures that leave
/// nothing to report.
///
/// [`CaseOutcome`]: crate::pipeline::CaseOutcome
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    /// The requested language is not in `executors.toml`.
    #[error("unknown language `{0}`")]
    UnknownLanguage(String),

    /// The language definition cannot run in this pipeline yet.
    #[error("language `{language}` cannot {step}: {reason}")]
    Unsupported {
        /// The language name.
        language: String,
        /// The step that is not supported.
        step: &'static str,
        /// Why the step is not supported.
        reason: String,
    },

    /// Writing the source, the input or the output files failed.
    #[error("the run directory could not be prepared: {0}")]
    Io(#[from] std::io::Error),

    /// The seccomp profile carries no file-access policy.
    ///
    /// The pipeline runs every submission under the ptrace supervisor, which
    /// only exists when the profile asks for it.
    #[error(
        "the seccomp profile carries no file policy, so there is no supervisor to run the submission under"
    )]
    NoFilePolicy,

    /// The runner refused the sandbox or failed to spawn the child.
    #[error(transparent)]
    Runner(#[from] rsjudge_runner::Error),

    /// A command template could not be rendered.
    #[error("rendering the {step} command failed: {message}")]
    Render {
        /// The step whose template failed.
        step: &'static str,
        /// The reason rendering failed.
        message: String,
    },

    /// The compiler did not finish before the compile timeout.
    #[error("the compile step did not finish within {0:?}")]
    CompileTimeout(Duration),

    /// The blocking execution task panicked.
    #[error("the execution task failed: {0}")]
    Task(String),
}
