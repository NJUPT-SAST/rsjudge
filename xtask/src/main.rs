use clap::Parser;
use xshell::{cmd, Shell};

#[derive(Debug, Parser)]
/// Build related tasks.
enum Command {
    /// Generate Rust modules from Protobuf definitions.
    Codegen,
    /// Package distribution-specific packages.
    Dist,
    /// Build Docker image.
    Docker,
}

fn main() -> anyhow::Result<()> {
    let command = Command::parse();
    let sh = Shell::new()?;
    if sh
        .current_dir()
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("rsjudge"))
    {
        match command {
            Command::Codegen => cmd!(sh, "echo Not implemented"),
            Command::Dist => cmd!(sh, "cargo deb"),
            Command::Docker => cmd!(sh, "docker build -t rsjudge ."),
        }
        .run()?
    } else {
        println!("Please run this command in the root directory of the project.")
    };
    Ok(())
}
