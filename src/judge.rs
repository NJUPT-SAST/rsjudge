// SPDX-License-Identifier: Apache-2.0

//! The `judge` and `run` commands.

use std::num::NonZeroU64;
use std::time::Duration;

use anyhow::Context as _;
use rsjudge_judger::pipeline::{
    CaseOutcome, CompileReport, ExecutionReport, Pipeline, Problem, ProblemOutcome, RunRequest,
    Verdict,
};

use crate::cli::{JudgeArgs, RunArgs};
use crate::config::Config;

/// Convert a millisecond argument to a limit, treating zero as unlimited.
fn millis_limit(value: Option<u64>) -> Option<Duration> {
    value
        .filter(|&milliseconds| milliseconds > 0)
        .map(Duration::from_millis)
}

/// Convert a kibibyte argument to a byte limit, treating zero as unlimited.
fn memory_limit(value: Option<u64>) -> Option<NonZeroU64> {
    value.and_then(|kilobytes| NonZeroU64::new(kilobytes.saturating_mul(1024)))
}

/// Compile `args.source` and judge it against every case of `args.problem`.
///
/// # Errors
///
/// Returns an error when the source cannot be read, when the problem cannot be
/// loaded, or when the pipeline cannot run the submission at all. A submission
/// that fails to compile, exits non-zero or produces a wrong output is
/// reported, not returned as an error.
pub async fn judge(config: &Config, args: &JudgeArgs) -> anyhow::Result<()> {
    let source = tokio::fs::read_to_string(&args.source)
        .await
        .with_context(|| format!("cannot read the source at {}", args.source.display()))?;
    let problem = Problem::load(&args.problem)
        .with_context(|| format!("cannot load the problem at {}", args.problem.display()))?;

    let pipeline = Pipeline::new(config.executors.clone(), config.seccomp.clone());
    let outcome = pipeline
        .judge_problem(&problem, &args.language, &source)
        .await?;

    print_problem(&outcome, args.json)?;

    Ok(())
}

/// Compile `args.source` and run it once, then print what it used.
///
/// # Errors
///
/// Returns an error when the source or the input cannot be read, or when the
/// pipeline cannot run the submission at all. A submission that fails to
/// compile or exits non-zero is reported, not returned as an error.
pub async fn run(config: &Config, args: &RunArgs) -> anyhow::Result<()> {
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
            time_limit: millis_limit(args.time_limit_ms),
            elapsed_limit: millis_limit(args.elapsed_limit_ms),
            memory_limit: memory_limit(args.memory_limit_kb),
        })
        .await?;

    print(&outcome, args.json)?;

    Ok(())
}

/// Print the outcome of one ad-hoc run.
#[allow(clippy::print_stdout)]
fn print(outcome: &CaseOutcome, json: bool) -> anyhow::Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(outcome)?);
        return Ok(());
    }

    println!("verdict: {}", outcome.verdict.as_str());

    if let Some(compile) = &outcome.compile {
        print_compile(compile);
    }

    if let Some(execution) = &outcome.execution {
        print_run(execution);
        print_syscalls(execution);
        print_output(execution);
    }

    Ok(())
}

/// Print the outcome of a submission judged against a whole problem.
#[allow(clippy::print_stdout)]
fn print_problem(outcome: &ProblemOutcome, json: bool) -> anyhow::Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(outcome)?);
        return Ok(());
    }

    println!("verdict: {}", outcome.verdict.as_str());

    if let Some(compile) = &outcome.compile {
        print_compile(compile);
    }

    for case in &outcome.cases {
        let score = if case.max_score > 0 {
            format!(" (score {}/{})", case.score, case.max_score)
        } else {
            String::new()
        };

        println!("case {}: {}{score}", case.index, case.verdict.as_str());

        if let Some(execution) = &case.execution {
            print_run(execution);

            if case.verdict != Verdict::Accepted && !execution.stderr.trim().is_empty() {
                println!("  stderr:");
                for line in execution.stderr.lines() {
                    println!("  {line}");
                }
            }
        }
    }

    for subtask in &outcome.subtasks {
        println!(
            "subtask {}: {} (score {}/{})",
            subtask.index,
            subtask.verdict.as_str(),
            subtask.score,
            subtask.max_score
        );
    }

    println!("score: {}/{}", outcome.score, outcome.total_score);

    Ok(())
}

/// Print what the compile or check step produced.
#[allow(clippy::print_stdout)]
fn print_compile(compile: &CompileReport) {
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

/// Print the status and the resources of one run.
#[allow(clippy::print_stdout)]
fn print_run(execution: &ExecutionReport) {
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
}

/// Print the syscall counters of one run.
#[allow(clippy::print_stdout)]
fn print_syscalls(execution: &ExecutionReport) {
    if execution.syscalls.is_empty() {
        println!("syscalls: none traced");
        return;
    }

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

/// Print what the run wrote to standard output and standard error.
#[allow(clippy::print_stdout)]
fn print_output(execution: &ExecutionReport) {
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
