// SPDX-License-Identifier: Apache-2.0

//! Abstraction for judger.

use std::{future::Future, path::Path, process::Output, time::Duration};

use indexmap::IndexMap;

use crate::language::{info::LanguageInfo, option::LanguageOption};

/// A trait for judging code.
pub trait Judger {
    /// The error type of the judger.
    type Error;

    /// Get a list of all supported languages.
    fn accept_languages(&self) -> IndexMap<String, LanguageInfo>;

    /// Execute the code of the specified language, with the given input and
    /// time limit.
    fn exec(
        &self,
        lang: &LanguageOption,
        code: &str,
        input: &str,
        time_limit: Duration,
    ) -> impl Future<Output = Result<Output, Self::Error>> + Send;

    /// Run the code of a specified language, with the given input and time
    /// limit, and compare the output with the answer.
    fn judge(
        &self,
        lang: &LanguageOption,
        code: &str,
        input_path: &Path,
        answer_path: &Path,
        time_limit: Duration,
    ) -> impl Future<Output = Result<(Output, JudgeResult), Self::Error>> + Send;
}

#[derive(Debug)]
/// The result of a judge.
///
/// This enum represents the result of a judge.
///
/// The meaning of each variant are as follows.
pub enum JudgeResult {
    /// The code is accepted.
    Accepted,
    /// The code failed to compile or failed to pass syntax check.
    CompileError,
    /// The code produced a wrong answer.
    WrongAnswer,
    /// The code produced a correct answer, but in a wrong format.
    PresentationError,
    /// The code produced a runtime error.
    RuntimeError,
    /// The code execution exceeded the time limit.
    TimeLimitExceeded,
    /// The code consumed more memory than the limit.
    MemoryLimitExceeded,
    /// The code's output size exceeded the specified output limit.
    OutputLimitExceeded,
}
