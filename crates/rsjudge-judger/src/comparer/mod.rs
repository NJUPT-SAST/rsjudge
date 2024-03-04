pub mod default_comparer;

use async_trait::async_trait;
use tokio::io::{self, AsyncRead};

#[derive(Debug, PartialEq)]
pub enum CompareResult {
    Accepted,
    WrongAnswer,
    PresentationError,
}

// TODO: Migrate to AsyncComparer trait
#[async_trait]
pub trait Comparer {
    async fn compare<Out, Ans>(&self, out: Out, ans: Ans) -> io::Result<CompareResult>
    where
        Out: AsyncRead + Send + Unpin,
        Ans: AsyncRead + Send + Unpin;
}
