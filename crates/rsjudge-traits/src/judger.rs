// SPDX-License-Identifier: Apache-2.0

use std::{path::Path, process::Output, time::Duration};

use async_trait::async_trait;

#[async_trait]
pub trait Judger {
    type Error;

    /// Get a list of all supported languages.
    fn accept_languages(&self) -> Vec<String>;

    /// Execute the code of the specified language, with the given input and time limit.
    async fn exec(
        &self,
        lang: &str,
        code: &str,
        input: &str,
        time_limit: Duration,
    ) -> Result<Output, Self::Error>;

    /// Run the code of a specified language, with the given input and time limit, and compare the output with the answer.
    async fn judge(
        &self,
        lang: &str,
        code: &str,
        input_path: &Path,
        answer_path: &Path,
        time_limit: Duration,
    ) -> Result<(Output, JudgeResult), Self::Error>;
}

pub enum JudgeResult {
    Accepted,
    CompileError,
    WrongAnswer,
    PresentationError,
    RuntimeError,
    TimeLimitExceeded,
    MemoryLimitExceeded,
    OutputLimitExceeded,
}
