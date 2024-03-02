use std::{
    future::Future,
    io,
    marker::PhantomPinned,
    pin::Pin,
    task::{ready, Context, Poll},
};

use pin_project::pin_project;
use tokio::io::AsyncRead;

use super::AsyncComparer;
use crate::CompareResult;

#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
#[pin_project]
pub struct Compare<'a, C: ?Sized, Out, Ans> {
    comparer: &'a mut C,
    out: Out,
    ans: Ans,

    // Make this future `!Unpin` for compatibility with async trait methods.
    #[pin]
    _pin: PhantomPinned,
}

pub(super) fn compare<'a, C, Out, Ans>(
    comparer: &'a mut C,
    out: Out,
    ans: Ans,
) -> Compare<'a, C, Out, Ans>
where
    C: AsyncComparer + Unpin + ?Sized,
    Out: AsyncRead + Unpin,
    Ans: AsyncRead + Unpin,
{
    Compare {
        comparer,
        out,
        ans,
        _pin: PhantomPinned,
    }
}

fn compare_internal<C, Out, Ans>(
    mut comparer: Pin<&mut C>,
    cx: &mut Context,
    out: Out,
    ans: Ans,
) -> Poll<io::Result<CompareResult>>
where
    C: AsyncComparer + Unpin + ?Sized,
    Out: AsyncRead + Unpin,
    Ans: AsyncRead + Unpin,
{
    Poll::Ready(ready!(comparer.as_mut().poll_compare(cx, out, ans)))
}

impl<'c, C, Out, Ans> Future for Compare<'c, C, Out, Ans>
where
    C: AsyncComparer + Unpin + ?Sized,
    Out: AsyncRead + Unpin,
    Ans: AsyncRead + Unpin,
{
    type Output = io::Result<CompareResult>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = self.project();

        compare_internal(Pin::new(*me.comparer), cx, me.out, me.ans)
    }
}
