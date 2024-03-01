use async_trait::async_trait;
use tokio::{
    io::{self, AsyncBufReadExt as _, AsyncRead, BufReader},
    join,
};
use tokio_stream::{wrappers::LinesStream, StreamExt as _};

use crate::{CompareResult, Comparer};

pub struct DefaultComparer {
    ignore_trailing_whitespace: bool,
    ignore_trailing_newline: bool,
}

impl DefaultComparer {
    pub fn new(ignore_trailing_whitespace: bool, ignore_trailing_newline: bool) -> Self {
        Self {
            ignore_trailing_whitespace,
            ignore_trailing_newline,
        }
    }

    fn compare_line(&self, out_line: &str, ans_line: &str) -> bool {
        let (out_line, ans_line) = if self.ignore_trailing_whitespace {
            (out_line.trim_end(), ans_line.trim_end())
        } else {
            (out_line, ans_line)
        };
        out_line == ans_line
    }
}

impl Default for DefaultComparer {
    fn default() -> Self {
        Self::new(true, true)
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

        // LinesStream::new(out.lines())
        let mut out_lines = LinesStream::new(out.lines());
        let mut ans_lines = LinesStream::new(ans.lines());

        while let (Some(out_line), Some(ans_line)) = join!(out_lines.next(), ans_lines.next()) {
            if !self.compare_line(&out_line?, &ans_line?) {
                return Ok(CompareResult::WrongAnswer);
            }
        }

        if self.ignore_trailing_newline {
            while let Some(out_line) = out_lines.next().await {
                if !out_line?.trim().is_empty() {
                    return Ok(CompareResult::WrongAnswer);
                }
            }

            while let Some(ans_line) = ans_lines.next().await {
                if !ans_line?.trim().is_empty() {
                    return Ok(CompareResult::WrongAnswer);
                }
            }
        }

        Ok(CompareResult::Accepted)
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

    use super::{CompareResult, Comparer as _, DefaultComparer};

    #[tokio::test]
    async fn compare_empty() -> io::Result<()> {
        let comparer = DefaultComparer::default();
        let out = empty();
        let ans = empty();
        let result = comparer.compare(out, ans).await?;
        assert_eq!(result, CompareResult::Accepted);
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
            let comparer = DefaultComparer::default();

            let result = comparer
                .compare(File::open(&out_path).await?, File::open(&ans_path).await?)
                .await?;
            assert_eq!(result, CompareResult::Accepted);
        }

        Ok(())
    }
}
