use std::path::PathBuf;

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use serde_json::from_str;
use xshell::{cmd, Shell};

pub(crate) mod deb;

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
    fn flag(self) -> &'static str {
        match self {
            Profile::Release => "--release",
            Profile::Debug => "",
        }
    }
}

pub(crate) fn build_script_out_dir(sh: &Shell, profile: Profile) -> anyhow::Result<PathBuf> {
    let pkgid = cmd!(sh, "cargo pkgid").read()?;
    let pkgid: Vec<_> = pkgid.split("#").collect();
    // dbg!(&pkgid);
    let flag = profile.flag();

    cmd!(sh, "cargo check {flag} --message-format=json")
        .read()?
        .lines()
        .find_map(|line| {
            let msg = from_str::<CargoCheckMessage>(line).expect("Not a valid message");
            match msg {
                CargoCheckMessage::BuildScriptExecuted {
                    package_id,
                    out_dir,
                } if package_id.contains(pkgid[0]) => Some(out_dir),
                _ => None,
            }
        })
        .ok_or(anyhow!("No build script executed."))
}
