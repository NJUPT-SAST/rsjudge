// SPDX-License-Identifier: Apache-2.0

//! Functions for working with [`std::process::Command`].

use std::{
    io::{self, ErrorKind},
    iter,
    process::{Command, Output, Stdio},
};

use thiserror::Error;

/// Display a command in a human-readable format, suitable for error messages.
///
/// # Examples
///
/// ```
/// use std::process::Command;
/// use rsjudge_utils::command::display_cmd;
///
/// let mut cmd = Command::new("echo");
/// cmd.arg("Hello, world!");
/// assert_eq!(display_cmd(&cmd), "echo 'Hello, world!'");
/// ```
#[must_use = "this function returns the formatted command"]
pub fn display_cmd(cmd: &Command) -> String {
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

/// Run a command, returning the output if succeeded, with some error handling.
///
/// # Examples
///
/// ```no_run
/// use std::process::Command;
/// use rsjudge_utils::command::check_output;
///
/// let mut cmd = Command::new("echo");
/// cmd.arg("Hello, world!");
/// let output = check_output(&mut cmd).unwrap();
/// assert_eq!(output.stdout, b"Hello, world!\n");
/// ```
///
/// # Errors
///
/// This function returns an error if the command was not found, failed to start,
/// or failed with a non-zero exit status.
pub fn check_output(cmd: &mut Command) -> Result<Output, ExecutionError> {
    let child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| match e.kind() {
            ErrorKind::NotFound => ExecutionError::NotFound {
                program: cmd.get_program().to_string_lossy().into_owned(),
            },
            _ => e.into(),
        })?;

    let output = child.wait_with_output()?;

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

    use crate::command::ExecutionError;

    #[test]
    fn command_not_found_with_custom_error() {
        let mut cmd = std::process::Command::new("nonexistent");
        let err = super::check_output(&mut cmd).unwrap_err();
        assert!(matches!(err, ExecutionError::NotFound { .. }));
    }

    #[test]
    fn command_failed_with_custom_error() {
        let mut cmd = std::process::Command::new("false");
        let err = super::check_output(&mut cmd).unwrap_err();

        assert!(matches!(err, ExecutionError::NonZeroExitStatus { .. }));
    }

    #[test]
    fn capture_output() {
        let mut cmd = std::process::Command::new("echo");
        cmd.arg("Hello, world!");
        let output = super::check_output(&mut cmd).unwrap();
        assert_eq!(output.stdout, b"Hello, world!\n");
    }
}
