pub mod compare;
use std::{
    pin::Pin,
    task::{Context, Poll},
};

use async_trait::async_trait;
use tokio::io::{self, AsyncRead};

use self::compare::{compare, Compare};

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

pub trait AsyncComparer {
    fn poll_compare<Out, Ans>(
        self: Pin<&mut Self>,
        cx: &mut Context,
        out: Out,
        ans: Ans,
    ) -> Poll<io::Result<CompareResult>>
    where
        Out: AsyncRead + Unpin,
        Ans: AsyncRead + Unpin;

    fn compare<'a, Out, Ans>(&'a mut self, out: Out, ans: Ans) -> Compare<'a, Self, Out, Ans>
    where
        Self: Unpin,
        Out: AsyncRead + Send + Unpin,
        Ans: AsyncRead + Send + Unpin,
    {
        compare(self, out, ans)
    }
}
