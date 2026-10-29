// SPDX-License-Identifier: Apache-2.0

//! Compile, run and report one submission.
//!
//! The pipeline is the first vertical slice of the judge: it renders the
//! command templates of a language, compiles the source, runs the program
//! once under the sandbox, and reports what the run used. It does not score,
//! and it runs a single case.
//!
//! ```no_run
//! # use std::collections::HashMap;
//! # use std::num::NonZeroU64;
//! # use std::time::Duration;
//! # use rsjudge_judger::pipeline::{Pipeline, RunRequest};
//! # use rsjudge_runner::SeccompConfig;
//! # use rsjudge_traits::language::config::LanguageDef;
//! # async fn run(
//! #     executors: HashMap<String, LanguageDef>,
//! #     seccomp: SeccompConfig,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! let pipeline = Pipeline::new(executors, seccomp);
//! let outcome = pipeline
//!     .run(RunRequest {
//!         language: "C".to_owned(),
//!         source: "int main(void) { return 0; }".to_owned(),
//!         stdin: None,
//!         time_limit: Duration::from_secs(1),
//!         elapsed_limit: Duration::from_secs(2),
//!         memory_limit: NonZeroU64::new(256 << 20),
//!     })
//!     .await?;
//!
//! println!("{:?}", outcome.verdict);
//! # Ok(())
//! # }
//! ```

pub mod error;
pub mod report;

mod execute;
mod render;
mod workspace;

use std::collections::HashMap;
use std::num::NonZeroU64;
use std::os::unix::process::ExitStatusExt as _;
use std::path::Path;
use std::time::{Duration, Instant};

pub use error::PipelineError;
pub use report::{CaseOutcome, CompileReport, ExecutionReport, SyscallCount, Verdict};
use rsjudge_runner::SeccompConfig;
use rsjudge_traits::language::config::LanguageDef;
use rsjudge_traits::language::option::LanguageOption;
use rsjudge_traits::resource::ResourceLimit;

use self::execute::{RunOutput, RunSpec};
use self::render::{RenderPaths, render};
use self::workspace::Workspace;

/// The signal the kernel sends a process that exceeded its CPU limit.
const SIGXCPU: i32 = 24;

/// How long a compiler may run before the pipeline gives up on it.
const DEFAULT_COMPILE_TIMEOUT: Duration = Duration::from_secs(10);

/// Everything one submission needs to be compiled and run.
#[derive(Debug, Clone)]
pub struct Pipeline {
    /// The language definitions, keyed by language name.
    executors: HashMap<String, LanguageDef>,

    /// The seccomp profile every run executes under.
    seccomp: SeccompConfig,

    /// How long the compile step may take.
    compile_timeout: Duration,
}

impl Pipeline {
    /// A pipeline over the languages of `executors.toml` and `seccomp.toml`.
    #[must_use]
    pub fn new(executors: HashMap<String, LanguageDef>, seccomp: SeccompConfig) -> Self {
        Self {
            executors,
            seccomp,
            compile_timeout: DEFAULT_COMPILE_TIMEOUT,
        }
    }

    /// Set how long the compile step may take.
    #[must_use]
    pub fn with_compile_timeout(mut self, timeout: Duration) -> Self {
        self.compile_timeout = timeout;
        self
    }

    /// Compile and run one submission.
    ///
    /// A submission that does not compile, exits non-zero or trips the policy
    /// yields a [`CaseOutcome`] rather than an error: only the failures that
    /// leave nothing to report become [`PipelineError`]s.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown language, a language this pipeline
    /// cannot run, a profile without a file policy, and failures of the
    /// sandbox itself.
    pub async fn run(&self, request: RunRequest) -> Result<CaseOutcome, PipelineError> {
        let def = self
            .executors
            .get(&request.language)
            .ok_or_else(|| PipelineError::UnknownLanguage(request.language.clone()))?;

        let Some(compile_template) = def.exec_type().compile() else {
            return Err(PipelineError::Unsupported {
                language: request.language.clone(),
                step: "compile",
                reason: "a source-code language has no compile step".to_owned(),
            });
        };

        if def.exec_type().execute().is_some() {
            return Err(PipelineError::Unsupported {
                language: request.language.clone(),
                step: "execute",
                reason: "the pipeline runs compiled binaries only so far".to_owned(),
            });
        }

        let workspace = Workspace::create()?;
        let source_name = format!("main.{}", source_extension(&request.language));
        let source = workspace.write(&source_name, request.source.as_bytes())?;
        let program = workspace.join("program");

        let paths = RenderPaths {
            src_file: &source,
            out_file: &program,
            tmp_dir: workspace.path(),
        };
        let option = LanguageOption {
            name: request.language.clone(),
            config: HashMap::new(),
        };

        let script = render(compile_template, def, &option, &paths).map_err(|message| {
            PipelineError::Render {
                step: "compile",
                message,
            }
        })?;

        let compile = self.compile(&workspace, &script).await?;
        if !compile.success {
            return Ok(CaseOutcome {
                verdict: Verdict::CompileError,
                compile: Some(compile),
                execution: None,
            });
        }

        let stdin = match &request.stdin {
            Some(input) => Some(workspace.write("stdin.txt", input.as_bytes())?),
            None => None,
        };
        let stdout = workspace.join("stdout.txt");
        let stderr = workspace.join("stderr.txt");

        let limits = ResourceLimit::new(
            Some(request.time_limit),
            Some(request.elapsed_limit),
            request.memory_limit,
            None,
        );

        let spec = RunSpec {
            program,
            args: Vec::new(),
            stdin,
            stdout: stdout.clone(),
            stderr: stderr.clone(),
            working_dir: workspace.path().to_path_buf(),
            limits,
            seccomp: self.seccomp.clone(),
            elapsed_limit: request.elapsed_limit,
        };

        let output = tokio::task::spawn_blocking(move || execute::run(spec))
            .await
            .map_err(|err| PipelineError::Task(err.to_string()))??;

        let (cpu_time_ms, max_rss_kb) = execute::usage_of(&output);
        let verdict = verdict_of(&output, request.memory_limit);

        let report = ExecutionReport {
            exit_code: output.exit.status.code(),
            signal: output.exit.status.signal(),
            timed_out: output.timed_out,
            elapsed_ms: millis(output.elapsed),
            cpu_time_ms,
            max_rss_kb,
            syscalls: output.syscalls,
            stdout: read_lossy(&stdout),
            stderr: read_lossy(&stderr),
        };

        Ok(CaseOutcome {
            verdict,
            compile: Some(compile),
            execution: Some(report),
        })
    }

    /// Run the rendered compile command in `workspace`.
    async fn compile(
        &self,
        workspace: &Workspace,
        script: &str,
    ) -> Result<CompileReport, PipelineError> {
        let start = Instant::now();

        let output = tokio::time::timeout(
            self.compile_timeout,
            tokio::process::Command::new("sh")
                .arg("-c")
                .arg(script)
                .current_dir(workspace.path())
                .output(),
        )
        .await;

        let output = match output {
            Err(_) => return Err(PipelineError::CompileTimeout(self.compile_timeout)),
            Ok(result) => result?,
        };

        Ok(CompileReport {
            success: output.status.success(),
            duration_ms: millis(start.elapsed()),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

/// One submission to compile and run.
#[derive(Debug, Clone)]
pub struct RunRequest {
    /// The language name, as it appears in `executors.toml`.
    pub language: String,

    /// The source code.
    pub source: String,

    /// The input to connect to standard input, when the case has one.
    pub stdin: Option<String>,

    /// The CPU time limit.
    pub time_limit: Duration,

    /// The total elapsed-time limit. Must be at least the CPU limit to make
    /// the CPU limit the one that reports first.
    pub elapsed_limit: Duration,

    /// The address-space limit, in bytes.
    pub memory_limit: Option<NonZeroU64>,
}

/// The verdict of a run the supervisor completed.
fn verdict_of(output: &RunOutput, memory_limit: Option<NonZeroU64>) -> Verdict {
    if output.timed_out {
        return Verdict::TimeLimitExceeded;
    }

    if output.exit.status.signal() == Some(SIGXCPU) {
        return Verdict::TimeLimitExceeded;
    }

    // The kernel does not report why an allocation failed, so an exit the
    // limit did not cause cannot be told apart from one it did. A run that
    // exited cleanly stays accepted even if its peak touched the limit.
    if let Some(limit) = memory_limit
        && output.exit.status.code() != Some(0)
        && output
            .exit
            .usage
            .is_some_and(|usage| usage.ram_usage().saturating_mul(1024) >= limit.get())
    {
        return Verdict::MemoryLimitExceeded;
    }

    match output.exit.status.code() {
        Some(0) => Verdict::Accepted,
        _ => Verdict::RuntimeError,
    }
}

/// The extension a language's source file needs so the compiler recognizes
/// it.
fn source_extension(language: &str) -> &'static str {
    match language.to_ascii_lowercase().as_str() {
        "c" => "c",
        "c++" | "cpp" | "cplusplus" => "cpp",
        "c#" | "csharp" | "cs" => "cs",
        "dart" => "dart",
        "go" | "golang" => "go",
        "java" => "java",
        "javascript" | "js" => "js",
        "kotlin" | "kt" => "kt",
        "python" | "py" => "py",
        "ruby" | "rb" => "rb",
        "rust" | "rs" => "rs",
        "swift" => "swift",
        "typescript" | "ts" => "ts",
        "zig" => "zig",
        _ => "txt",
    }
}

/// Read a file the run wrote, replacing invalid UTF-8 instead of failing.
fn read_lossy(path: &Path) -> String {
    std::fs::read(path).map_or_else(
        |_| String::new(),
        |bytes| String::from_utf8_lossy(&bytes).into_owned(),
    )
}

/// Convert a duration to whole milliseconds, saturating on overflow.
fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;
    use std::time::Duration;

    use rsjudge_runner::{ResourceUsage, SupervisedExit};

    use super::{SIGXCPU, source_extension, verdict_of};
    use crate::pipeline::Verdict;
    use crate::pipeline::execute::RunOutput;

    #[test]
    fn knows_the_source_extensions_of_the_demo_languages() {
        assert_eq!(source_extension("C"), "c");
        assert_eq!(source_extension("C++"), "cpp");
        assert_eq!(source_extension("Rust"), "rs");
        assert_eq!(source_extension("python"), "py");
        assert_eq!(source_extension("unknown"), "txt");
    }

    #[test]
    fn extension_matching_ignores_case() {
        assert_eq!(source_extension("c++"), "cpp");
        assert_eq!(source_extension("JAVA"), "java");
    }

    #[test]
    fn a_run_stopped_at_the_limit_reports_the_time_limit() {
        let mut output = run_output(true, None, Some(0));
        output.timed_out = true;

        assert_eq!(verdict_of(&output, None), Verdict::TimeLimitExceeded);
    }

    #[test]
    fn a_killed_run_reports_a_runtime_error() {
        let output = run_output(false, None, Some(11));

        assert_eq!(verdict_of(&output, None), Verdict::RuntimeError);
    }

    #[test]
    fn sigxcpu_reports_the_time_limit() {
        let output = run_output(false, None, Some(SIGXCPU));

        assert_eq!(verdict_of(&output, None), Verdict::TimeLimitExceeded);
    }

    #[test]
    fn peak_memory_at_the_limit_reports_the_memory_limit() {
        let output = run_output(true, Some(1024), Some(11));
        let limit = NonZeroU64::new(1 << 20);

        assert_eq!(verdict_of(&output, limit), Verdict::MemoryLimitExceeded);
    }

    #[test]
    fn a_clean_exit_at_the_limit_stays_accepted() {
        let output = run_output(true, Some(1024), Some(0));
        let limit = NonZeroU64::new(1 << 20);

        assert_eq!(verdict_of(&output, limit), Verdict::Accepted);
    }

    #[test]
    fn a_zero_exit_is_accepted() {
        let output = run_output(true, Some(4), Some(0));

        assert_eq!(verdict_of(&output, None), Verdict::Accepted);
    }

    fn run_output(exited_normally: bool, rss_kb: Option<u64>, number: Option<i32>) -> RunOutput {
        use std::os::unix::process::ExitStatusExt as _;

        let status = if exited_normally {
            std::process::ExitStatus::from_raw(number.unwrap_or(0) << 8)
        } else {
            std::process::ExitStatus::from_raw(number.unwrap_or(0))
        };

        let usage = rss_kb.map(|rss| ResourceUsage::new(Duration::ZERO, rss));

        RunOutput {
            exit: SupervisedExit { status, usage },
            timed_out: false,
            elapsed: Duration::from_millis(1),
            syscalls: Vec::new(),
        }
    }
}
