//! A default comparer implementation, supporting ignoring trailing whitespace and/or trailing newline.

use async_trait::async_trait;
use tokio::{
    io::{self, AsyncBufReadExt as _, AsyncRead, BufReader},
    join,
};
use tokio_stream::{wrappers::SplitStream, StreamExt as _};

use crate::{utils::trim::slice::trim_ascii_end, CompareResult, Comparer};

///
pub struct DefaultComparer {
    ignore_trailing_whitespace: bool,
    ignore_trailing_newline: bool,
}

impl DefaultComparer {
    pub const fn new(ignore_trailing_whitespace: bool, ignore_trailing_newline: bool) -> Self {
        Self {
            ignore_trailing_whitespace,
            ignore_trailing_newline,
        }
    }

    pub const fn common() -> Self {
        Self::new(true, true)
    }

    pub const fn exact_match() -> Self {
        Self::new(false, false)
    }

    fn compare_line(&self, out_line: &[u8], ans_line: &[u8]) -> bool {
        let (out_line, ans_line) = if self.ignore_trailing_whitespace {
            (trim_ascii_end(out_line), trim_ascii_end(ans_line))
        } else {
            (out_line, ans_line)
        };
        out_line == ans_line
    }
}

impl Default for DefaultComparer {
    fn default() -> Self {
        Self::common()
    }
}

#[async_trait]
impl Comparer for DefaultComparer {
    async fn compare<Out, Ans>(&self, out: Out, ans: Ans) -> io::Result<CompareResult>
    where
        Out: AsyncRead + Send + Unpin,
        Ans: AsyncRead + Send + Unpin,
    {
        let out = BufReader::new(out);
        let ans = BufReader::new(ans);

        // TODO: Replace this with `read_until` to avoid unnecessary allocations, and deal with trailing line endings.
        let mut out_lines = SplitStream::new(out.split(b'\n')).fuse();
        let mut ans_lines = SplitStream::new(ans.split(b'\n')).fuse();
        loop {
            match join!(out_lines.next(), ans_lines.next()) {
                (Some(out_line), Some(ans_line)) => {
                    if !self.compare_line(&out_line?, &ans_line?) {
                        return Ok(CompareResult::WrongAnswer);
                    }
                }
                (Some(out_line), _) => {
                    if !self.ignore_trailing_newline || !self.compare_line(&out_line?, &[]) {
                        return Ok(CompareResult::WrongAnswer);
                    }
                }
                (_, Some(ans_line)) => {
                    if !self.ignore_trailing_newline || !self.compare_line(&[], &ans_line?) {
                        return Ok(CompareResult::WrongAnswer);
                    }
                }
                _ => return Ok(CompareResult::Accepted),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use temp_dir::TempDir;
    use tokio::{
        fs::File,
        io::{empty, AsyncWriteExt as _},
    };

    use super::{CompareResult, DefaultComparer};
    use crate::Comparer as _;

    #[tokio::test]
    async fn compare_empty() -> io::Result<()> {
        let common_comparer = DefaultComparer::common();
        let common_result = common_comparer.compare(empty(), empty()).await?;
        assert_eq!(common_result, CompareResult::Accepted);
        let exact_comparer = DefaultComparer::exact_match();
        let exact_result = exact_comparer.compare(empty(), empty()).await?;
        assert_eq!(exact_result, CompareResult::Accepted);
        Ok(())
    }

    #[tokio::test]
    async fn compare_files() -> io::Result<()> {
        let temp_dir = TempDir::new()?;

        let out_path = temp_dir.path().join("out");
        let ans_path = temp_dir.path().join("ans");

        {
            File::create(&out_path)
                .await?
                .write_all(b"Hello, World!\n")
                .await?;
            File::create(&ans_path)
                .await?
                .write_all(b"Hello, World!\n")
                .await?;
        }

        {
            let comparer = DefaultComparer::common();
            let result = comparer
                .compare(File::open(&out_path).await?, File::open(&ans_path).await?)
                .await?;
            assert_eq!(result, CompareResult::Accepted);
        }

        Ok(())
    }

    #[tokio::test]
    async fn compare_with_trailing_whitespace() -> io::Result<()> {
        let out = b"Hello, World! \n";
        let ans = b"Hello, World!\n";
        let common_comparer = DefaultComparer::common();
        let common_result = common_comparer.compare(&out[..], &ans[..]).await?;
        assert_eq!(common_result, CompareResult::Accepted);
        let exact_comparer = DefaultComparer::exact_match();
        let exact_result = exact_comparer.compare(&out[..], &ans[..]).await?;
        assert_eq!(exact_result, CompareResult::WrongAnswer);
        Ok(())
    }

    #[tokio::test]
    async fn compare_with_invalid_utf8() -> io::Result<()> {
        let out = b"Hello, World! \xFF\n";
        let ans = b"Hello, World! \xFF\n";
        let common_comparer = DefaultComparer::common();
        let common_result = common_comparer.compare(&out[..], &ans[..]).await?;
        assert_eq!(common_result, CompareResult::Accepted);
        let exact_comparer = DefaultComparer::exact_match();
        let exact_result = exact_comparer.compare(&out[..], &ans[..]).await?;
        assert_eq!(exact_result, CompareResult::Accepted);
        Ok(())
    }

    #[tokio::test]
    async fn compare_with_trailing_newline() -> io::Result<()> {
        let out = b"Hello, World!\n";
        let ans = b"Hello, World!";
        let common_comparer = DefaultComparer::common();
        let common_result = common_comparer.compare(&out[..], &ans[..]).await?;
        assert_eq!(common_result, CompareResult::Accepted);
        let exact_comparer = DefaultComparer::exact_match();
        let exact_result = exact_comparer.compare(&out[..], &ans[..]).await?;
        assert_eq!(exact_result, CompareResult::WrongAnswer);
        Ok(())
    }

    #[tokio::test]
    async fn compare_with_trailing_content_after_newline() -> io::Result<()> {
        let out = b"Hello, World!\naaa\n";
        let ans = b"Hello, World!";
        let common_comparer = DefaultComparer::common();
        let common_result = common_comparer.compare(&out[..], &ans[..]).await?;
        assert_eq!(common_result, CompareResult::WrongAnswer);
        let exact_comparer = DefaultComparer::exact_match();
        let exact_result = exact_comparer.compare(&out[..], &ans[..]).await?;
        assert_eq!(exact_result, CompareResult::WrongAnswer);
        Ok(())
    }
}
