//! Functions for working with [`std::process::Command`].

use std::{
    io::ErrorKind,
    iter,
    process::{Command, Output},
};

use anyhow::{bail, ensure};

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
/// assert_eq!(display_cmd(&cmd), "\"echo\" \"Hello, world!\"");
/// ```
pub fn display_cmd(cmd: &Command) -> String {
    let args = iter::once(cmd.get_program())
        .chain(cmd.get_args())
        .map(|arg| arg.to_string_lossy());

    shell_words::join(args)
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
pub fn check_output(cmd: &mut Command) -> anyhow::Result<Output> {
    let output = match cmd.output() {
        Ok(o) => o,
        Err(e) => match e.kind() {
            ErrorKind::NotFound => bail!(
                "`{}` not found. Please install it or check your PATH.",
                cmd.get_program().to_string_lossy()
            ),
            _ => Err(e)?,
        },
    };

    ensure!(
        output.status.success(),
        "`{:?}` failed with: {:#?}",
        display_cmd(cmd),
        output
    );

    Ok(output)
}
