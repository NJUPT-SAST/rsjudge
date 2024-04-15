// SPDX-License-Identifier: Apache-2.0

mod default_comparer;

use std::io;

use async_trait::async_trait;
use tokio::io::AsyncRead;

pub use self::default_comparer::DefaultComparer;

#[derive(Debug, PartialEq, Eq)]
pub enum CompareResult {
    Accepted,
    WrongAnswer,
    PresentationError,
}

#[async_trait]
pub trait Comparer {
    async fn compare<Out, Ans>(&self, out: Out, ans: Ans) -> io::Result<CompareResult>
    where
        Out: AsyncRead + Send + Unpin,
        Ans: AsyncRead + Send + Unpin;
}
