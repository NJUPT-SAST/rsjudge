use xshell::Shell;

#[cfg(unix)]
pub(crate) fn deb_package(sh: Shell) -> anyhow::Result<()> {
    use xshell::cmd;

    use crate::dist::prepare_out_dir;

    cmd!(sh, "cargo build --release").run()?;

    prepare_out_dir(&sh)?;

    cmd!(sh, "cargo deb -v").run()?;

    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn deb_package(sh: Shell) -> anyhow::Result<()> {
    use anyhow::anyhow;

    Err(anyhow!("Not supported on non-unix platforms."))
}
