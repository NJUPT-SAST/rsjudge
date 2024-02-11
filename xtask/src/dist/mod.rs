use std::path::PathBuf;

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use serde_json::from_str;
use xshell::{cmd, Shell};

pub(crate) mod deb;
pub(crate) mod rpm;

#[derive(Deserialize, Debug, Serialize)]
#[serde(tag = "reason", rename_all = "kebab-case")]
pub(crate) enum CargoCheckMessage {
    CompilerArtifact,
    BuildScriptExecuted {
        package_id: String,
        out_dir: PathBuf,
    },
    BuildFinished,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Profile {
    Release,
    Debug,
}

impl Profile {
    fn flag(self) -> Option<&'static str> {
        match self {
            Profile::Release => Some("--release"),
            Profile::Debug => None,
        }
    }
}

/// Get the `OUT_DIR` directory path of a specified build profile. This will run `cargo build` under the hood.
// TODO: This function should be run only once during CI build process.
pub(crate) fn build_script_out_dir(sh: &Shell, profile: Profile) -> anyhow::Result<PathBuf> {
    let pkgid = cmd!(sh, "cargo pkgid").read()?;
    let pkgid = pkgid
        .split_once("#")
        .expect(&format!("Unexpected pkgid: {:?}", pkgid));

    let flag = profile.flag();

    cmd!(sh, "cargo build --locked {flag...} --message-format=json")
        .read()?
        .lines()
        .find_map(|line| {
            let msg = from_str::<CargoCheckMessage>(line).expect("Not a valid message");
            match msg {
                CargoCheckMessage::BuildScriptExecuted {
                    package_id,
                    out_dir,
                } if package_id.contains(pkgid.0) => Some(out_dir),
                _ => None,
            }
        })
        .ok_or(anyhow!("No build script executed."))
}

#[cfg(unix)]
fn prepare_out_dir(sh: &Shell) -> Result<(), anyhow::Error> {
    const OUT_DIR: &'static str = "target/release/out";

    use std::{fs::remove_dir_all, os::unix::fs::symlink};

    let build_script_out_dir = build_script_out_dir(sh, Profile::Release)?;

    let _ = remove_dir_all(OUT_DIR);

    symlink(build_script_out_dir, OUT_DIR)?;
    Ok(())
}
