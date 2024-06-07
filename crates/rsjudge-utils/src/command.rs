// SPDX-License-Identifier: Apache-2.0

//! Functions for working with [`tokio::process::Command`].

use std::{
    io::{self, ErrorKind},
    iter,
    process::{ExitStatus, Output, Stdio},
};

use thiserror::Error;
use tokio::process::Command;

/// Display a command in a human-readable format, suitable for error messages.
///
/// # Examples
///
/// ```
/// use tokio::process::Command;
/// use rsjudge_utils::command::display_cmd;
///
/// let mut cmd = Command::new("echo");
/// cmd.arg("Hello, world!");
/// assert_eq!(display_cmd(&cmd), "echo 'Hello, world!'");
/// ```
#[must_use = "this function returns the formatted command"]
pub fn display_cmd(cmd: &Command) -> String {
    let cmd = cmd.as_std();
    let args = iter::once(cmd.get_program())
        .chain(cmd.get_args())
        .map(|arg| arg.to_string_lossy());

    shell_words::join(args)
}

/// Error type for failed command execution.
#[derive(Debug, Error)]
pub enum ExecutionError {
    /// The requested command was not found.
    #[error("Program `{program}` not found. Please install it or check your PATH.")]
    NotFound {
        /// The command name.
        program: String,
    },

    /// The command failed with an error.
    #[error("Command failed: {0}")]
    Failed(#[from] io::Error),

    /// The command failed with a non-zero exit status.
    #[error("Command `{command}` failed with: {:?}", .output)]
    /// Error type for a non-zero exit status.
    NonZeroExitStatus {
        /// The command that failed.
        command: String,
        /// The output of the command.
        output: Output,
    },
}

impl ExecutionError {
    /// Get the output of the command, if any.
    ///
    /// This method will return `Some(&Output)` if the error is `NonZeroExitStatus`, otherwise `None`.
    #[must_use]
    pub fn output(&self) -> Option<&Output> {
        match self {
            Self::NonZeroExitStatus { output, .. } => Some(output),
            _ => None,
        }
    }

    /// Get the exit status of the command, if any.
    #[must_use]
    pub fn exit_status(&self) -> Option<ExitStatus> {
        match self {
            Self::NonZeroExitStatus { output, .. } => Some(output.status),
            _ => None,
        }
    }
}

/// Run a command, returning the output if succeeded, with some error handling.
///
/// # Examples
///
/// ```no_run
/// use tokio::process::Command;
/// use rsjudge_utils::command::check_output;
/// # #[tokio::main]
/// # async fn main() {
/// let mut cmd = Command::new("echo");
/// cmd.arg("Hello, world!");
/// let output = check_output(&mut cmd).await.unwrap();
/// assert_eq!(output.stdout, b"Hello, world!\n");
/// # }
/// ```
///
/// # Errors
///
/// This function returns an error if the command was not found, failed to start,
/// or failed with a non-zero exit status.
pub async fn check_output(cmd: &mut Command) -> Result<Output, ExecutionError> {
    let child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| match e.kind() {
            ErrorKind::NotFound => ExecutionError::NotFound {
                program: cmd.as_std().get_program().to_string_lossy().into_owned(),
            },
            _ => e.into(),
        })?;

    let output = child.wait_with_output().await?;

    if output.status.success() {
        Ok(output)
    } else {
        Err(ExecutionError::NonZeroExitStatus {
            command: display_cmd(cmd),
            output,
        })
    }
}

#[cfg(test)]
mod tests {

    use tokio::process::Command;

    use crate::command::{check_output, ExecutionError};

    #[tokio::test]
    #[ignore = "execute `nonexistent` on the platform"]
    async fn command_not_found_with_custom_error() {
        let mut cmd = Command::new("nonexistent");
        let err = check_output(&mut cmd).await.unwrap_err();
        assert_eq!(
            err.to_string(),
            ExecutionError::NotFound {
                program: "nonexistent".to_string()
            }
            .to_string()
        );
    }

    #[tokio::test]
    #[ignore = "execute `false` on the platform"]
    async fn command_failed_with_custom_error() {
        let mut cmd = Command::new("false");
        let err = check_output(&mut cmd).await.unwrap_err();

        dbg!(&err);

        assert!(matches!(err, ExecutionError::NonZeroExitStatus { .. }));

        let code = err.exit_status().and_then(|status| status.code());

        assert_eq!(code, Some(1));
    }

    #[tokio::test]
    #[ignore = "execute `echo` on the platform"]
    async fn capture_output() {
        let mut cmd = Command::new("echo");
        cmd.arg("Hello, world!");
        let output = check_output(&mut cmd).await.unwrap();
        assert_eq!(output.stdout, b"Hello, world!\n");
    }
}
