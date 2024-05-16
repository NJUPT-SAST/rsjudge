// SPDX-License-Identifier: Apache-2.0

//! Abstraction for judger.

use std::{path::Path, process::Output, time::Duration};

use async_trait::async_trait;
use indexmap::IndexMap;

use crate::language::info::LanguageInfo;

#[async_trait]
/// A trait for judging code.
pub trait Judger {
    /// The error type of the judger.
    type Error;

    /// Get a list of all supported languages.
    fn accept_languages(&self) -> IndexMap<String, LanguageInfo>;

    /// Execute the code of the specified language, with the given input and time limit.
    async fn exec(
        &self,
        lang: &LanguageInfo,
        code: &str,
        input: &str,
        time_limit: Duration,
    ) -> Result<Output, Self::Error>;

    /// Run the code of a specified language, with the given input and time limit, and compare the output with the answer.
    async fn judge(
        &self,
        lang: &LanguageInfo,
        code: &str,
        input_path: &Path,
        answer_path: &Path,
        time_limit: Duration,
    ) -> Result<(Output, JudgeResult), Self::Error>;
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
