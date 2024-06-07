// SPDX-License-Identifier: Apache-2.0

//! A default comparer implementation, supporting ignoring trailing whitespace and/or trailing newline.

use std::io;

use async_trait::async_trait;
use futures::try_join;
use rsjudge_utils::trim_ascii_end;
use tokio::io::{AsyncBufReadExt as _, AsyncRead, BufReader};

use crate::{CompareResult, Comparer};

/// A default comparer implementation with basic configurations.
#[must_use = "Comparer makes no sense if it is not used"]
pub struct DefaultComparer {
    case_sensitive: bool,
    ignore_trailing_whitespace: bool,
    ignore_trailing_newline: bool,
}

impl DefaultComparer {
    pub const fn new(
        case_sensitive: bool,
        ignore_trailing_whitespace: bool,
        ignore_trailing_newline: bool,
    ) -> Self {
        Self {
            case_sensitive,
            ignore_trailing_whitespace,
            ignore_trailing_newline,
        }
    }

    pub const fn common() -> Self {
        Self::new(true, true, true)
    }

    pub const fn exact_match() -> Self {
        Self::new(true, false, false)
    }

    fn compare_line(&self, out_line: &[u8], ans_line: &[u8]) -> bool {
        let (out, ans) = match (out_line, ans_line) {
            ([out @ .., b'\n'], [ans @ .., b'\n']) => (out, ans),
            (out, [ans @ .., b'\n']) | ([out @ .., b'\n'], ans) => {
                if !self.ignore_trailing_newline {
                    return false;
                }
                (out, ans)
            }
            (out, ans) => (out, ans),
        };

        let (out, ans) = if self.ignore_trailing_whitespace {
            (trim_ascii_end(out), trim_ascii_end(ans))
        } else {
            (out, ans)
        };

        if self.case_sensitive {
            out == ans
        } else {
            out.eq_ignore_ascii_case(ans)
        }
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
        let mut out = BufReader::new(out);
        let mut ans = BufReader::new(ans);

        let mut out_buf = Vec::new();
        let mut ans_buf = Vec::new();

        loop {
            // The "line" read by `read_until` includes the delimiter, i.e., the `b'\n'` byte.
            // This is important since a trailing newline need to be detected,
            // so we can perform exact match when needed.
            let (out_len, ans_len) = try_join!(
                out.read_until(b'\n', &mut out_buf),
                ans.read_until(b'\n', &mut ans_buf),
            )?;

            if out_len == 0 && ans_len == 0 {
                return Ok(CompareResult::Accepted);
            }

            if self.compare_line(&out_buf, &ans_buf) {
                out_buf.clear();
                ans_buf.clear();
            } else {
                return Ok(CompareResult::WrongAnswer);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use tempfile::TempDir;
    use tokio::{
        fs::File,
        io::{empty, AsyncWriteExt as _},
    };

    use crate::{CompareResult, Comparer as _, DefaultComparer};

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
                .write_all(b"Hello, world!\n")
                .await?;
            File::create(&ans_path)
                .await?
                .write_all(b"Hello, world!\n")
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
        let out = b"Hello, world! \n";
        let ans = b"Hello, world!\n";
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
        let out = b"Hello, world! \xFF\n";
        let ans = b"Hello, world! \xFF\n";
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
        let out = b"Hello, world!\n";
        let ans = b"Hello, world!";
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
        let out = b"Hello, world!\naaa\n";
        let ans = b"Hello, world!";
        let common_comparer = DefaultComparer::common();
        let common_result = common_comparer.compare(&out[..], &ans[..]).await?;
        assert_eq!(common_result, CompareResult::WrongAnswer);
        let exact_comparer = DefaultComparer::exact_match();
        let exact_result = exact_comparer.compare(&out[..], &ans[..]).await?;
        assert_eq!(exact_result, CompareResult::WrongAnswer);
        Ok(())
    }

    #[tokio::test]
    async fn compare_case_sensitive() -> io::Result<()> {
        let out = b"Hello, World!";
        let ans = b"Hello, world!";

        let common_comparer = DefaultComparer::common();
        let common_result = common_comparer.compare(&out[..], &ans[..]).await?;
        assert_eq!(common_result, CompareResult::WrongAnswer);

        let exact_comparer = DefaultComparer::exact_match();
        let exact_result = exact_comparer.compare(&out[..], &ans[..]).await?;
        assert_eq!(exact_result, CompareResult::WrongAnswer);

        let case_insensitive_comparer = DefaultComparer::new(false, true, true);
        let case_insensitive_result = case_insensitive_comparer
            .compare(&out[..], &ans[..])
            .await?;
        assert_eq!(case_insensitive_result, CompareResult::Accepted);

        Ok(())
    }
}
