use xshell::Shell;
#[cfg(unix)]
pub(crate) fn deb_package(sh: Shell) -> anyhow::Result<()> {
    use std::{fs::remove_dir_all, os::unix::fs::symlink, path::PathBuf};

    use xshell::cmd;

    use crate::dist::{build_script_out_dir, Profile};

    cmd!(sh, "cargo build --release").run()?;
    let build_script_out_dir = build_script_out_dir(&sh, Profile::Release)?;

    // Link out_dir to target/out, so that cargo-deb can find it.
    remove_dir_all(PathBuf::from("target/release/out")).ok();
    symlink(build_script_out_dir, "target/release/out")?;
    cmd!(sh, "cargo deb").run()?;

    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn deb_package(sh: Shell) -> anyhow::Result<()> {
    use anyhow::anyhow;

    Err(anyhow!("Not supported on non-unix platforms."))
}
