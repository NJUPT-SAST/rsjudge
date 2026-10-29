// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(author, about, version)]
pub struct Args {
    #[arg(short, long, default_value = "./config", value_name = "DIR")]
    /// Specify the configuration directory
    pub config_dir: PathBuf,

    /// The command to run. Without one, the server only loads its
    /// configuration and reports the environment it detected.
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// The command the binary runs.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Compile one submission, run it once, and report what it used.
    Judge(JudgeArgs),
}

/// The arguments of the `judge` command.
#[derive(Debug, clap::Args)]
pub struct JudgeArgs {
    /// The language name, as it appears in `executors.toml`.
    #[arg(long, default_value = "C")]
    pub language: String,

    /// The source file to compile and run.
    #[arg(long, value_name = "FILE")]
    pub source: PathBuf,

    /// The file to connect to standard input.
    #[arg(long, value_name = "FILE")]
    pub stdin: Option<PathBuf>,

    /// The CPU time limit, in milliseconds.
    #[arg(long, default_value_t = 1000)]
    pub time_limit_ms: u64,

    /// The total elapsed-time limit of the run, in milliseconds.
    #[arg(long, default_value_t = 2000)]
    pub elapsed_limit_ms: u64,

    /// The address-space limit, in kibibytes. Zero disables the limit.
    #[arg(long, default_value_t = 262_144)]
    pub memory_limit_kb: u64,

    /// Print the report as JSON instead of human-readable text.
    #[arg(long)]
    pub json: bool,
}
