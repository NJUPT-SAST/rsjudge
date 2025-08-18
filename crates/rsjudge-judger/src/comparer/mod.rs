// SPDX-License-Identifier: Apache-2.0

mod default_comparer;

use std::future::Future;
use std::io;

use tokio::io::AsyncRead;

pub use self::default_comparer::DefaultComparer;

#[derive(Debug, PartialEq, Eq)]
pub enum CompareResult {
    Accepted,
    WrongAnswer,
    PresentationError,
}

pub trait Comparer {
    fn compare<Out, Ans>(
        &self,
        out: Out,
        ans: Ans,
    ) -> impl Future<Output = io::Result<CompareResult>> + Send
    where
        Out: AsyncRead + Send + Unpin,
        Ans: AsyncRead + Send + Unpin;
}
