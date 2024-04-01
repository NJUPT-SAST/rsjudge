use std::{
    io::ErrorKind,
    process::{Command, Output},
};

use anyhow::{bail, ensure};

/// Display a command in a human-readable format, suitable for error messages.
pub fn display_cmd(cmd: &Command) -> String {
    let mut s = format!("{:?}", cmd.get_program().to_string_lossy());
    s.extend(
        cmd.get_args()
            .map(|arg| format!(" {:?}", arg.to_string_lossy())),
    );
    s
}

/// Run a command, returning the output if succeeded, with some error handling.
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
