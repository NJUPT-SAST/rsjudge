//! A default comparer implementation, supporting ignoring trailing whitespace and/or trailing newline.

use std::io;

use async_trait::async_trait;
use futures::try_join;
use rsjudge_utils::trim::trim_ascii_end;
use tokio::io::{AsyncBufReadExt as _, AsyncRead, BufReader};

use crate::{CompareResult, Comparer};

/// A default comparer implementation with basic configurations.
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

        out == ans
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
    use std::{any::type_name, io};

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

    #[test]
    fn test() {
        println!("{}", type_name::<DefaultComparer>());
    }
}
