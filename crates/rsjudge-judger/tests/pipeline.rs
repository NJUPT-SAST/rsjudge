// SPDX-License-Identifier: Apache-2.0

//! End-to-end tests of the pipeline: compile a fixture, run it under the
//! sandbox, and inspect the report.

use std::collections::HashMap;
use std::num::NonZeroU64;
use std::process::Command;
use std::time::Duration;

use rsjudge_judger::pipeline::{CaseOutcome, Pipeline, RunRequest, Verdict};
use rsjudge_runner::SeccompConfig;
use rsjudge_traits::language::config::LanguageDef;
use serde_json::json;

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
        time_limit: TIME_LIMIT,
        elapsed_limit: ELAPSED_LIMIT,
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
async fn a_compile_error_reports_the_compiler_output() {
    if !gcc_available() {
        eprintln!("skipping: gcc is not available");
        return;
    }

    let outcome = run(&pipeline(permissive_seccomp()), fixture("broken.c")).await;

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
    request.time_limit = Duration::from_secs(10);
    request.elapsed_limit = Duration::from_millis(300);

    let outcome = pipeline(permissive_seccomp())
        .run(request)
        .await
        .expect("the pipeline failed");

    assert_eq!(outcome.verdict, Verdict::TimeLimitExceeded);

    let execution = outcome.execution.expect("no execution report");
    assert!(execution.timed_out, "the supervisor did not stop the run");
}
