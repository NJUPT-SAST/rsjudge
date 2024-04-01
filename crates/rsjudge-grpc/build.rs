use std::{env, io::BufRead, path::PathBuf, process::Command};

use rsjudge_utils::command::check_output;
use tonic_build::configure;

/// Generate Rust code from the proto files.
///
/// The build script uses `buf` to list all the proto files in the `proto` directory and then
/// compiles them using `tonic_build`.
///
/// `buf` is needed to run this build script.
fn main() -> anyhow::Result<()> {
    let proto_out_dir = {
        let mut out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
        out_dir.push("proto");
        out_dir
    };

    let buf_ls_files = check_output(Command::new("buf").current_dir("proto").arg("ls-files"))?;

    let protos = buf_ls_files
        .stdout
        .lines()
        .filter_map(|line| line.ok().filter(|s| !s.is_empty()));

    check_output(
        Command::new("buf")
            .args(["export", "proto", "-o"])
            .arg(&proto_out_dir),
    )?;

    for proto in protos {
        configure()
            .emit_rerun_if_changed(false)
            .compile(&[proto_out_dir.join(proto)], &[&proto_out_dir])?;
    }

    println!("cargo:rerun-if-changed=proto");
    println!("cargo:rerun-if-changed=build.rs");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_cmd() {
        let mut cmd = Command::new("echo");
        cmd.arg("hello");
        assert_eq!(display_cmd(&cmd), r#""echo" "hello""#);
    }
}
