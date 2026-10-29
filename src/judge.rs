// SPDX-License-Identifier: Apache-2.0

//! The `judge` command: compile and run one submission.

use std::num::NonZeroU64;
use std::time::Duration;

use anyhow::Context as _;
use rsjudge_judger::pipeline::{CaseOutcome, Pipeline, RunRequest};

use crate::cli::JudgeArgs;
use crate::config::Config;

/// Compile and run the submission `args` names, then print what it used.
///
/// # Errors
///
/// Returns an error when the source or the input cannot be read, or when the
/// pipeline cannot run the submission at all. A submission that fails to
/// compile or exits non-zero is reported, not returned as an error.
pub async fn run(config: &Config, args: &JudgeArgs) -> anyhow::Result<()> {
    let source = tokio::fs::read_to_string(&args.source)
        .await
        .with_context(|| format!("cannot read the source at {}", args.source.display()))?;

    let stdin = match &args.stdin {
        Some(path) => Some(
            tokio::fs::read_to_string(path)
                .await
                .with_context(|| format!("cannot read the input at {}", path.display()))?,
        ),
        None => None,
    };

    let pipeline = Pipeline::new(config.executors.clone(), config.seccomp.clone());

    let outcome = pipeline
        .run(RunRequest {
            language: args.language.clone(),
            source,
            stdin,
            time_limit: Duration::from_millis(args.time_limit_ms),
            elapsed_limit: Duration::from_millis(args.elapsed_limit_ms),
            memory_limit: NonZeroU64::new(args.memory_limit_kb.saturating_mul(1024)),
        })
        .await?;

    print(&outcome, args.json)?;

    Ok(())
}

/// Print the outcome of a run.
#[allow(clippy::print_stdout)]
fn print(outcome: &CaseOutcome, json: bool) -> anyhow::Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(outcome)?);
        return Ok(());
    }

    println!("verdict: {}", outcome.verdict.as_str());

    if let Some(compile) = &outcome.compile {
        println!(
            "compile: {} in {} ms",
            if compile.success { "ok" } else { "failed" },
            compile.duration_ms
        );
        for (name, text) in [("stdout", &compile.stdout), ("stderr", &compile.stderr)] {
            if !text.trim().is_empty() {
                println!("compile {name}:");
                print!("{text}");
                if !text.ends_with('\n') {
                    println!();
                }
            }
        }
    }

    if let Some(execution) = &outcome.execution {
        let status = match (execution.exit_code, execution.signal) {
            (Some(code), _) => format!("exit {code}"),
            (None, Some(signal)) => format!("signal {signal}"),
            (None, None) => "unknown status".to_owned(),
        };

        println!(
            "run: {status}, elapsed {} ms, cpu {} ms, peak rss {} KiB{}",
            execution.elapsed_ms,
            execution.cpu_time_ms,
            execution.max_rss_kb,
            if execution.timed_out {
                ", stopped at the time limit"
            } else {
                ""
            }
        );

        if execution.syscalls.is_empty() {
            println!("syscalls: none traced");
        } else {
            println!("syscalls:");
            for syscall in &execution.syscalls {
                let mut details = Vec::new();
                if syscall.denied > 0 {
                    details.push(format!("{} denied", syscall.denied));
                }
                if syscall.killed > 0 {
                    details.push(format!("{} killed", syscall.killed));
                }
                let details = if details.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", details.join(", "))
                };

                println!("  {} {}{details}", syscall.name, syscall.count);
            }
            println!(
                "denied calls: {}, killed calls: {}",
                execution.denied_calls(),
                execution.killed_calls()
            );
        }

        for (name, text) in [("stdout", &execution.stdout), ("stderr", &execution.stderr)] {
            if !text.is_empty() {
                println!("{name}:");
                print!("{text}");
                if !text.ends_with('\n') {
                    println!();
                }
            }
        }
    }

    Ok(())
}
