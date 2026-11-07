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
    /// Judge one submission against every case of a problem.
    Judge(JudgeArgs),

    /// Compile one submission and run it once.
    Run(RunArgs),
}

/// The arguments of the `judge` command.
#[derive(Debug, clap::Args)]
pub struct JudgeArgs {
    /// The `.tar.zst` problem archive.
    #[arg(long, value_name = "FILE")]
    pub problem: PathBuf,

    /// The source file to compile and judge.
    #[arg(long, value_name = "FILE")]
    pub source: PathBuf,

    /// The language name, as it appears in `executors.toml`.
    #[arg(long, value_name = "NAME")]
    pub language: String,

    /// Print the report as JSON instead of human-readable text.
    #[arg(long)]
    pub json: bool,
}

/// The arguments of the `run` command.
#[derive(Debug, clap::Args)]
pub struct RunArgs {
    /// The language name, as it appears in `executors.toml`.
    #[arg(long, value_name = "NAME")]
    pub language: String,

    /// The source file to compile and run.
    #[arg(long, value_name = "FILE")]
    pub source: PathBuf,

    /// The file to connect to standard input.
    #[arg(long, value_name = "FILE")]
    pub stdin: Option<PathBuf>,

    /// The CPU time limit, in milliseconds. Unset or zero means unlimited.
    #[arg(long, value_name = "MS")]
    pub time_limit_ms: Option<u64>,

    /// The total elapsed-time limit, in milliseconds. Unset or zero means
    /// unlimited.
    #[arg(long, value_name = "MS")]
    pub elapsed_limit_ms: Option<u64>,

    /// The address-space limit, in kibibytes. Unset or zero means unlimited.
    #[arg(long, value_name = "KB")]
    pub memory_limit_kb: Option<u64>,

    /// Print the report as JSON instead of human-readable text.
    #[arg(long)]
    pub json: bool,
}
