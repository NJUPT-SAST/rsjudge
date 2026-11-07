// SPDX-License-Identifier: Apache-2.0

//! End-to-end tests of the pipeline: compile a fixture, run it under the
//! sandbox, and inspect the report.

use std::collections::HashMap;
use std::num::NonZeroU64;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use rsjudge_judger::pipeline::{
    CaseOutcome, Pipeline, PipelineError, Problem, RunRequest, Verdict,
};
use rsjudge_runner::SeccompConfig;
use rsjudge_traits::language::config::LanguageDef;
use serde_json::json;
use tempfile::TempDir;

/// How long a compiler may take in these tests.
const COMPILE_TIMEOUT: Duration = Duration::from_secs(20);

/// The CPU time limit of the ordinary cases.
const TIME_LIMIT: Duration = Duration::from_secs(1);

/// The total elapsed-time limit of the ordinary cases.
const ELAPSED_LIMIT: Duration = Duration::from_secs(5);

/// The address-space limit of the ordinary cases.
const MEMORY_LIMIT: NonZeroU64 = NonZeroU64::new(256 << 20).unwrap();

fn gcc_available() -> bool {
    Command::new("gcc")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn executors() -> HashMap<String, LanguageDef> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../config-demo/executors.toml"
    );
    let text = std::fs::read_to_string(path).expect("cannot read executors.toml");

    toml::from_str(&text).expect("cannot parse executors.toml")
}

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));

    std::fs::read_to_string(path).expect("cannot read the fixture")
}

fn permissive_seccomp() -> SeccompConfig {
    serde_json::from_value(json!({
        "defaultAction": { "action": "SCMP_ACT_ALLOW" },
        "filePolicy": {
            "defaultAction": "allow",
            "rules": [
                {
                    "paths": ["/"],
                    "match": "prefix",
                    "access": "any",
                    "action": "allow",
                },
            ],
        },
    }))
    .expect("invalid permissive profile")
}

fn denying_seccomp() -> SeccompConfig {
    serde_json::from_value(json!({
        "defaultAction": { "action": "SCMP_ACT_ALLOW" },
        "filePolicy": {
            "defaultAction": "allow",
            "rules": [
                {
                    "paths": ["/etc/shadow"],
                    "match": "exact",
                    "action": "deny",
                },
                {
                    "paths": ["/"],
                    "match": "prefix",
                    "access": "any",
                    "action": "allow",
                },
            ],
        },
    }))
    .expect("invalid denying profile")
}

fn pipeline(seccomp: SeccompConfig) -> Pipeline {
    Pipeline::new(executors(), seccomp).with_compile_timeout(COMPILE_TIMEOUT)
}

fn request(source: String) -> RunRequest {
    RunRequest {
        language: "C".to_owned(),
        source,
        stdin: None,
        time_limit: Some(TIME_LIMIT),
        elapsed_limit: Some(ELAPSED_LIMIT),
        memory_limit: Some(MEMORY_LIMIT),
    }
}

async fn run(pipeline: &Pipeline, source: String) -> CaseOutcome {
    pipeline
        .run(request(source))
        .await
        .expect("the pipeline failed")
}

#[tokio::test]
async fn a_hello_world_compiles_runs_and_reports() {
    if !gcc_available() {
        eprintln!("skipping: gcc is not available");
        return;
    }

    let mut request = request(fixture("hello.c"));
    request.stdin = Some("rsjudge\n".to_owned());

    let outcome = pipeline(permissive_seccomp())
        .run(request)
        .await
        .expect("the pipeline failed");

    assert_eq!(outcome.verdict, Verdict::Accepted);

    let compile = outcome.compile.expect("no compile report");
    assert!(compile.success, "the compiler failed: {}", compile.stderr);

    let execution = outcome.execution.expect("no execution report");
    assert_eq!(execution.exit_code, Some(0));
    assert_eq!(execution.stdout, "hello, rsjudge\n");
    assert!(execution.max_rss_kb > 0, "the kernel reported no peak RSS");
    assert!(
        execution.elapsed_ms >= execution.cpu_time_ms,
        "the run spent less elapsed time than CPU time"
    );

    // The supervisor only stops on the syscalls that name a path, so the
    // loader's opens and the program's `fgets` show up while plain `read` and
    // `write` calls — which take a descriptor, not a path — stay untraced.
    let served = |names: &[&str]| {
        execution
            .syscalls
            .iter()
            .any(|syscall| names.contains(&syscall.name.as_str()) && syscall.count > 0)
    };
    assert!(
        served(&["openat", "open"]),
        "no open-family call: {:?}",
        execution.syscalls
    );
    assert!(
        served(&["newfstatat", "fstatat", "statx", "stat"]),
        "no stat-family call: {:?}",
        execution.syscalls
    );
    assert_eq!(execution.denied_calls(), 0);
    assert_eq!(execution.killed_calls(), 0);
}

#[tokio::test]
async fn an_unlimited_run_still_reports() {
    if !gcc_available() {
        eprintln!("skipping: gcc is not available");
        return;
    }

    let mut request = request(fixture("hello.c"));
    request.stdin = Some("rsjudge\n".to_owned());
    request.time_limit = None;
    request.elapsed_limit = None;
    request.memory_limit = None;

    let outcome = pipeline(permissive_seccomp())
        .run(request)
        .await
        .expect("the unlimited run failed");

    assert_eq!(outcome.verdict, Verdict::Accepted);
    assert_eq!(
        outcome.execution.expect("no execution report").stdout,
        "hello, rsjudge\n"
    );
}

#[tokio::test]
async fn a_compile_error_reports_the_compiler_output() {
    if !gcc_available() {
        eprintln!("skipping: gcc is not available");
        return;
    }

    let outcome = run(&pipeline(permissive_seccomp()), fixture("broken.c.invalid")).await;

    assert_eq!(outcome.verdict, Verdict::CompileError);

    let compile = outcome.compile.expect("no compile report");
    assert!(!compile.success);
    assert!(
        compile.stderr.contains("error"),
        "the compiler reported nothing: {}",
        compile.stderr
    );
    assert!(outcome.execution.is_none(), "a broken program ran");
}

#[tokio::test]
async fn a_denied_path_fails_the_open() {
    if !gcc_available() {
        eprintln!("skipping: gcc is not available");
        return;
    }

    let outcome = run(&pipeline(denying_seccomp()), fixture("denied.c")).await;

    assert_eq!(outcome.verdict, Verdict::RuntimeError);

    let execution = outcome.execution.expect("no execution report");
    assert_eq!(execution.exit_code, Some(42));
    assert!(
        execution.denied_calls() > 0,
        "the policy denied nothing: {:?}",
        execution.syscalls
    );
    assert!(
        execution.stdout.contains("failed the open"),
        "the program did not see the denial: {}",
        execution.stdout
    );
}

#[tokio::test]
async fn a_spinning_program_hits_the_elapsed_limit() {
    if !gcc_available() {
        eprintln!("skipping: gcc is not available");
        return;
    }

    let mut request = request(fixture("spin.c"));
    request.time_limit = Some(Duration::from_secs(10));
    request.elapsed_limit = Some(Duration::from_millis(300));

    let outcome = pipeline(permissive_seccomp())
        .run(request)
        .await
        .expect("the pipeline failed");

    assert_eq!(outcome.verdict, Verdict::TimeLimitExceeded);

    let execution = outcome.execution.expect("no execution report");
    assert!(execution.timed_out, "the supervisor did not stop the run");
}

fn python_available() -> bool {
    Command::new("python3")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

const SIMPLE_PROBLEM: &str = r#"
score = 100

[judge]
judgeType = "classic"

[resourceLimits]
time = 1000
memory = 256

[task]
taskType = "simple"

[[task.cases]]
input = "1.in"
answer = "1.ans"

[[task.cases]]
input = "2.in"
answer = "2.ans"
"#;

fn write_archive(dir: &TempDir, name: &str, files: &[(&str, &str)]) -> PathBuf {
    let path = dir.path().join(name);
    let file = std::fs::File::create(&path).expect("failed to create the problem archive");
    let encoder =
        zstd::stream::write::Encoder::new(file, 0).expect("failed to start the compressor");
    let mut archive = tar::Builder::new(encoder);

    for (entry_name, contents) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(u64::try_from(contents.len()).expect("the entry fits in a tar header"));
        header.set_mode(0o644);
        header.set_cksum();
        archive
            .append_data(&mut header, entry_name, contents.as_bytes())
            .expect("failed to append an archive entry");
    }

    let encoder = archive.into_inner().expect("failed to finish the archive");
    encoder.finish().expect("failed to finish the compression");

    path
}

fn simple_problem(dir: &TempDir) -> PathBuf {
    write_archive(
        dir,
        "problem.tar.zst",
        &[
            ("problem.toml", SIMPLE_PROBLEM),
            ("1.in", "rsjudge\n"),
            ("1.ans", "hello, rsjudge\n"),
            ("2.in", "rsjudge\n"),
            ("2.ans", "goodbye, rsjudge\n"),
        ],
    )
}

#[tokio::test]
async fn a_problem_runs_every_case_and_scores_the_submission() {
    if !gcc_available() {
        eprintln!("skipping: gcc is not available");
        return;
    }

    let dir = TempDir::new().expect("failed to create the problem directory");
    let archive = simple_problem(&dir);

    let problem = Problem::load(&archive).expect("failed to load the problem");
    let outcome = pipeline(permissive_seccomp())
        .judge_problem(&problem, "C", &fixture("hello.c"))
        .await
        .expect("the problem run failed");

    assert_eq!(outcome.verdict, Verdict::WrongAnswer);
    assert_eq!(outcome.score, 50);
    assert_eq!(outcome.total_score, 100);
    assert_eq!(outcome.cases.len(), 2);

    assert_eq!(outcome.cases[0].index, 1);
    assert_eq!(outcome.cases[0].verdict, Verdict::Accepted);
    assert_eq!(outcome.cases[0].score, 50);
    assert_eq!(outcome.cases[0].max_score, 50);

    assert_eq!(outcome.cases[1].index, 2);
    assert_eq!(outcome.cases[1].verdict, Verdict::WrongAnswer);
    assert_eq!(outcome.cases[1].score, 0);
    assert!(outcome.subtasks.is_empty());
}

#[tokio::test]
async fn a_failed_case_zeroes_its_subtask() {
    if !gcc_available() {
        eprintln!("skipping: gcc is not available");
        return;
    }

    let dir = TempDir::new().expect("failed to create the problem directory");
    let archive = write_archive(
        &dir,
        "problem.tar.zst",
        &[
            (
                "problem.toml",
                r#"
score = 100

[judge]
judgeType = "classic"

[resourceLimits]
time = 1000
memory = 256

[task]
taskType = "subtask"

[[task.subtasks]]
score = 100

[[task.subtasks.cases]]
input = "1.in"
answer = "1.ans"

[[task.subtasks.cases]]
input = "2.in"
answer = "2.ans"
"#,
            ),
            ("1.in", "rsjudge\n"),
            ("1.ans", "hello, rsjudge\n"),
            ("2.in", "rsjudge\n"),
            ("2.ans", "goodbye, rsjudge\n"),
        ],
    );

    let problem = Problem::load(&archive).expect("failed to load the problem");
    let outcome = pipeline(permissive_seccomp())
        .judge_problem(&problem, "C", &fixture("hello.c"))
        .await
        .expect("the problem run failed");

    assert_eq!(outcome.verdict, Verdict::WrongAnswer);
    assert_eq!(outcome.score, 0);
    assert_eq!(outcome.cases.len(), 2);
    assert_eq!(outcome.cases[0].verdict, Verdict::Accepted);
    assert_eq!(outcome.cases[1].verdict, Verdict::WrongAnswer);

    assert_eq!(outcome.subtasks.len(), 1);
    assert_eq!(outcome.subtasks[0].verdict, Verdict::WrongAnswer);
    assert_eq!(outcome.subtasks[0].score, 0);
    assert_eq!(outcome.subtasks[0].max_score, 100);
}

#[tokio::test]
async fn a_broken_submission_scores_zero_without_running() {
    if !gcc_available() {
        eprintln!("skipping: gcc is not available");
        return;
    }

    let dir = TempDir::new().expect("failed to create the problem directory");
    let archive = simple_problem(&dir);

    let problem = Problem::load(&archive).expect("failed to load the problem");
    let outcome = pipeline(permissive_seccomp())
        .judge_problem(&problem, "C", &fixture("broken.c.invalid"))
        .await
        .expect("the problem run failed");

    assert_eq!(outcome.verdict, Verdict::CompileError);
    assert_eq!(outcome.score, 0);
    assert!(outcome.cases.is_empty(), "a broken submission ran a case");
}

#[tokio::test]
async fn a_source_code_language_checks_and_runs_the_source() {
    if !python_available() {
        eprintln!("skipping: python3 is not available");
        return;
    }

    let dir = TempDir::new().expect("failed to create the problem directory");
    let archive = write_archive(
        &dir,
        "problem.tar.zst",
        &[
            (
                "problem.toml",
                r#"
score = 100

[judge]
judgeType = "classic"

[resourceLimits]
time = 1000
memory = 256

[task]
taskType = "simple"

[[task.cases]]
input = "1.in"
answer = "1.ans"
"#,
            ),
            ("1.in", "rsjudge\n"),
            ("1.ans", "hello, rsjudge\n"),
        ],
    );

    let problem = Problem::load(&archive).expect("failed to load the problem");
    let source = "name = input()\nprint(f\"hello, {name}\")\n";
    let outcome = pipeline(permissive_seccomp())
        .judge_problem(&problem, "Python", source)
        .await
        .expect("the problem run failed");

    assert_eq!(outcome.verdict, Verdict::Accepted);
    assert_eq!(outcome.score, 100);
    assert_eq!(outcome.cases.len(), 1);
    assert_eq!(outcome.cases[0].verdict, Verdict::Accepted);

    let compile = outcome.compile.expect("no check report");
    assert!(compile.success, "the check failed: {}", compile.stderr);
}

#[tokio::test]
async fn a_byte_code_language_compiles_then_runs_the_runtime() {
    let dir = TempDir::new().expect("failed to create the problem directory");
    let archive = write_archive(
        &dir,
        "problem.tar.zst",
        &[
            (
                "problem.toml",
                r#"
score = 100

[judge]
judgeType = "classic"

[resourceLimits]
time = 1000
memory = 256

[task]
taskType = "simple"

[[task.cases]]
input = "1.in"
answer = "1.ans"
"#,
            ),
            ("1.in", "rsjudge\n"),
            ("1.ans", "rsjudge\n"),
        ],
    );

    let mut executors = executors();
    let def: LanguageDef = toml::from_str(
        r#"
exec_type = "byte_code"
compile = "cp {src_file} {out_file}"
execute = "sh {out_file}"
out_extension = "sh"
"#,
    )
    .expect("invalid language definition");
    executors.insert("ByteCodeTest".to_owned(), def);

    let problem = Problem::load(&archive).expect("failed to load the problem");
    let pipeline = Pipeline::new(executors, permissive_seccomp());
    let outcome = pipeline
        .judge_problem(&problem, "ByteCodeTest", "cat\n")
        .await
        .expect("the problem run failed");

    assert_eq!(outcome.verdict, Verdict::Accepted);
    assert_eq!(outcome.score, 100);

    let compile = outcome.compile.expect("no compile report");
    assert!(compile.success, "the compile failed: {}", compile.stderr);
}

#[tokio::test]
async fn a_missing_runtime_is_a_configuration_error() {
    let dir = TempDir::new().expect("failed to create the problem directory");
    let archive = write_archive(
        &dir,
        "problem.tar.zst",
        &[
            (
                "problem.toml",
                r#"
score = 100

[judge]
judgeType = "classic"

[resourceLimits]
time = 1000
memory = 256

[task]
taskType = "simple"

[[task.cases]]
input = "1.in"
answer = "1.ans"
"#,
            ),
            ("1.in", ""),
            ("1.ans", ""),
        ],
    );

    let mut executors = executors();
    let def: LanguageDef = toml::from_str(
        r#"
exec_type = "source_code"
execute = "rsjudge-missing-runtime-xyz {src_file}"
"#,
    )
    .expect("invalid language definition");
    executors.insert("Missing".to_owned(), def);

    let problem = Problem::load(&archive).expect("failed to load the problem");
    let pipeline = Pipeline::new(executors, permissive_seccomp());
    let error = pipeline
        .judge_problem(&problem, "Missing", "print(1)")
        .await
        .expect_err("a missing runtime was accepted");

    assert!(
        matches!(error, PipelineError::MissingRuntime { .. }),
        "the pipeline returned another error: {error}"
    );
}
