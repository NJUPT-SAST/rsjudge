// SPDX-License-Identifier: Apache-2.0

//! Compile, run and report a submission.
//!
//! The pipeline renders the command templates of a language, compiles the
//! source, runs the program under the sandbox and reports what the run used.
//! [`Pipeline::run`] covers one ad-hoc case; [`Pipeline::judge_problem`]
//! compiles once, runs every case of a [`Problem`], compares each output with
//! its answer and adds up the score.
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
//!         time_limit: Some(Duration::from_secs(1)),
//!         elapsed_limit: Some(Duration::from_secs(2)),
//!         memory_limit: NonZeroU64::new(256 << 20),
//!     })
//!     .await?;
//!
//! println!("{:?}", outcome.verdict);
//! # Ok(())
//! # }
//! ```

pub mod error;
pub mod problem;
pub mod report;

mod execute;
mod render;
mod workspace;

use std::collections::HashMap;
use std::num::NonZeroU64;
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::process::ExitStatusExt as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub use error::PipelineError;
pub use problem::{Problem, ProblemCase, ProblemError, ProblemLimits, ProblemSubtask, ProblemTask};
pub use report::{
    CaseOutcome, CaseResult, CompileReport, ExecutionReport, ProblemOutcome, SubtaskResult,
    SyscallCount, Verdict,
};
use rsjudge_runner::SeccompConfig;
use rsjudge_traits::language::config::{ExecType, LanguageDef};
use rsjudge_traits::language::option::LanguageOption;
use rsjudge_traits::resource::ResourceLimit;

use self::execute::{RunOutput, RunSpec};
use self::render::{RenderPaths, render};
use self::workspace::Workspace;
use crate::comparer::{CompareResult, Comparer as _, DefaultComparer};
use crate::judger::request::cases::JudgeType;

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

/// A submission that compiled and is ready to run.
#[derive(Debug)]
struct CompiledSubmission {
    /// The directory holding the source, the artifact and the case files.
    workspace: Workspace,

    /// The program to execute.
    program: PathBuf,

    /// The arguments passed to the program.
    args: Vec<String>,

    /// What the compile or check step produced, when the language has one.
    compile: Option<CompileReport>,
}

/// What the compile or check step of a submission produced.
#[derive(Debug)]
enum CompileOutcome {
    /// The submission compiled and can run.
    Compiled(CompiledSubmission),

    /// The compiler or the syntax check rejected the submission.
    Failed(CompileReport),
}

/// One case run of a compiled submission.
#[derive(Debug)]
struct CaseExecution {
    /// What the run used.
    report: ExecutionReport,

    /// The file holding the raw standard output.
    stdout: PathBuf,

    /// The verdict derived from the exit status alone.
    verdict: Verdict,
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

    /// Compile and run one ad-hoc submission.
    ///
    /// A submission that does not compile, exits non-zero or trips the policy
    /// yields a [`CaseOutcome`] rather than an error: only the failures that
    /// leave nothing to report become [`PipelineError`]s.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown language, a missing runtime, a profile
    /// without a file policy, and failures of the sandbox itself.
    pub async fn run(&self, request: RunRequest) -> Result<CaseOutcome, PipelineError> {
        let compiled = match self
            .compile_submission(&request.language, &request.source)
            .await?
        {
            CompileOutcome::Compiled(compiled) => compiled,
            CompileOutcome::Failed(report) => {
                return Ok(CaseOutcome {
                    verdict: Verdict::CompileError,
                    compile: Some(report),
                    execution: None,
                });
            }
        };

        let limits = ProblemLimits {
            time: request.time_limit,
            elapsed: request.elapsed_limit,
            memory: request.memory_limit,
        };
        let execution = self
            .execute_case(
                &compiled,
                request.stdin.as_deref().map(str::as_bytes),
                limits,
                "run",
            )
            .await?;

        Ok(CaseOutcome {
            verdict: execution.verdict,
            compile: compiled.compile.clone(),
            execution: Some(execution.report),
        })
    }

    /// Compile one submission and judge it against every case of `problem`.
    ///
    /// The submission is compiled once; every case then runs under the
    /// problem's limits, its output is compared with its answer, and the
    /// scores of the simple cases or subtasks are added up.
    ///
    /// The cases share the workspace of the compile step, which the demo
    /// profile allows the submission to write; the per-run layout of #548
    /// gives each run its own private directory.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown language, an unsupported judge type,
    /// a missing runtime, a profile without a file policy, a case file that
    /// cannot be read, and failures of the sandbox itself. A submission that
    /// does not compile is reported as a [`ProblemOutcome`], not an error.
    pub async fn judge_problem(
        &self,
        problem: &Problem,
        language: &str,
        source: &str,
    ) -> Result<ProblemOutcome, PipelineError> {
        if !matches!(problem.judge(), JudgeType::Classic) {
            return Err(PipelineError::Unsupported {
                language: language.to_owned(),
                step: "judge",
                reason: "the pipeline compares output with the answer only; special judge and interactive judging are not wired in yet".to_owned(),
            });
        }

        let compiled = match self.compile_submission(language, source).await? {
            CompileOutcome::Compiled(compiled) => compiled,
            CompileOutcome::Failed(report) => {
                return Ok(ProblemOutcome {
                    verdict: Verdict::CompileError,
                    compile: Some(report),
                    cases: Vec::new(),
                    subtasks: Vec::new(),
                    score: 0,
                    total_score: problem.total_score(),
                });
            }
        };

        let limits = problem.limits();
        let mut case_results = Vec::new();
        let mut subtask_results = Vec::new();
        let mut score = 0;

        match problem.task() {
            ProblemTask::Simple { cases } => {
                for (index, case) in cases.iter().enumerate() {
                    let result = self.judge_case(&compiled, case, index + 1, limits).await?;
                    score += result.score;
                    case_results.push(result);
                }
            }

            ProblemTask::Subtask { subtasks } => {
                for (subtask_index, subtask) in subtasks.iter().enumerate() {
                    let mut verdict = Verdict::Accepted;

                    for case in &subtask.cases {
                        let index = case_results.len() + 1;
                        let result = self.judge_case(&compiled, case, index, limits).await?;

                        if result.verdict != Verdict::Accepted && verdict == Verdict::Accepted {
                            verdict = result.verdict;
                        }

                        case_results.push(result);
                    }

                    let earned = if verdict == Verdict::Accepted {
                        subtask.score
                    } else {
                        0
                    };
                    score += earned;
                    subtask_results.push(SubtaskResult {
                        index: subtask_index + 1,
                        verdict,
                        score: earned,
                        max_score: subtask.score,
                    });
                }
            }
        }

        let verdict = case_results
            .iter()
            .find(|case| case.verdict != Verdict::Accepted)
            .map_or(Verdict::Accepted, |case| case.verdict);

        Ok(ProblemOutcome {
            verdict,
            compile: compiled.compile.clone(),
            cases: case_results,
            subtasks: subtask_results,
            score,
            total_score: problem.total_score(),
        })
    }

    /// Compile `source` in the language `language`.
    async fn compile_submission(
        &self,
        language: &str,
        source: &str,
    ) -> Result<CompileOutcome, PipelineError> {
        let def = self
            .executors
            .get(language)
            .ok_or_else(|| PipelineError::UnknownLanguage(language.to_owned()))?;

        let workspace = Workspace::create()?;
        let source_name = def.src_rename().map_or_else(
            || format!("main.{}", source_extension(language)),
            ToOwned::to_owned,
        );
        let source_path = workspace.write(&source_name, source.as_bytes())?;

        let artifact_name = def.out_rename().map_or_else(
            || match def.out_extension() {
                Some(extension) => format!("program.{extension}"),
                None => "program".to_owned(),
            },
            ToOwned::to_owned,
        );
        let out_file = workspace.join(&artifact_name);

        let option = LanguageOption {
            name: language.to_owned(),
            config: HashMap::new(),
        };
        let paths = RenderPaths {
            src_file: &source_path,
            out_file: &out_file,
            tmp_dir: workspace.path(),
        };

        // Resolve the runtime before the compile step, so a missing runtime
        // is reported as a configuration error even when the source would
        // fail to compile.
        let execution = match def.exec_type() {
            ExecType::Binary { .. } => None,
            ExecType::ByteCode { execute, .. } | ExecType::SourceCode { execute, .. } => {
                Some(execute_command(execute, def, &option, &paths)?)
            }
        };

        // A source-code language checks the source instead of compiling it.
        let (template, step) = match (def.exec_type().compile(), def.exec_type().check()) {
            (Some(template), _) => (Some(template), "compile"),
            (None, Some(template)) => (Some(template), "check"),
            (None, None) => (None, "compile"),
        };

        let compile = match template {
            Some(template) => {
                let script = render(template, def, &option, &paths)
                    .map_err(|message| PipelineError::Render { step, message })?;
                let report = self.compile(&workspace, &script).await?;

                if !report.success {
                    return Ok(CompileOutcome::Failed(report));
                }

                Some(report)
            }
            None => None,
        };

        let (program, args) = match (def.exec_type(), execution) {
            (ExecType::Binary { .. }, _) => {
                ensure_artifact(&out_file)?;
                (out_file, Vec::new())
            }

            (ExecType::ByteCode { .. }, Some(execution)) => {
                ensure_artifact(&out_file)?;
                execution
            }

            (ExecType::SourceCode { .. }, Some(execution)) => execution,

            (ExecType::ByteCode { .. } | ExecType::SourceCode { .. }, None) => {
                unreachable!("a language with an execute template rendered its runtime")
            }
        };

        Ok(CompileOutcome::Compiled(CompiledSubmission {
            workspace,
            program,
            args,
            compile,
        }))
    }

    /// Run one case of a compiled submission.
    async fn judge_case(
        &self,
        compiled: &CompiledSubmission,
        case: &ProblemCase,
        index: usize,
        limits: ProblemLimits,
    ) -> Result<CaseResult, PipelineError> {
        let input =
            tokio::fs::read(&case.input)
                .await
                .map_err(|source| PipelineError::ProblemIo {
                    path: case.input.clone(),
                    source,
                })?;
        let label = format!("case-{index}");
        let execution = self
            .execute_case(compiled, Some(&input), limits, &label)
            .await?;

        let verdict = if execution.verdict == Verdict::Accepted {
            self.compare(&execution.stdout, &case.answer).await?
        } else {
            execution.verdict
        };
        let score = if verdict == Verdict::Accepted {
            case.score
        } else {
            0
        };

        Ok(CaseResult {
            index,
            verdict,
            score,
            max_score: case.score,
            execution: Some(execution.report),
        })
    }

    /// Run `compiled` once under the sandbox.
    async fn execute_case(
        &self,
        compiled: &CompiledSubmission,
        stdin: Option<&[u8]>,
        limits: ProblemLimits,
        label: &str,
    ) -> Result<CaseExecution, PipelineError> {
        let stdin = match stdin {
            Some(bytes) => Some(compiled.workspace.write(&format!("{label}.in"), bytes)?),
            None => None,
        };
        let stdout = compiled.workspace.join(&format!("{label}.out"));
        let stderr = compiled.workspace.join(&format!("{label}.err"));

        let spec = RunSpec {
            program: compiled.program.clone(),
            args: compiled.args.clone(),
            stdin,
            stdout: stdout.clone(),
            stderr: stderr.clone(),
            working_dir: compiled.workspace.path().to_path_buf(),
            limits: ResourceLimit::new(limits.time, limits.elapsed, limits.memory, None),
            seccomp: self.seccomp.clone(),
            elapsed_limit: limits.elapsed,
        };

        let output = tokio::task::spawn_blocking(move || execute::run(spec))
            .await
            .map_err(|err| PipelineError::Task(err.to_string()))??;

        let (cpu_time_ms, max_rss_kb) = execute::usage_of(&output);
        let verdict = verdict_of(&output, limits.memory);
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

        Ok(CaseExecution {
            report,
            stdout,
            verdict,
        })
    }

    /// Compare the raw output of a case with its answer.
    async fn compare(&self, output: &Path, answer: &Path) -> Result<Verdict, PipelineError> {
        let out =
            tokio::fs::File::open(output)
                .await
                .map_err(|source| PipelineError::ProblemIo {
                    path: output.to_owned(),
                    source,
                })?;
        let ans =
            tokio::fs::File::open(answer)
                .await
                .map_err(|source| PipelineError::ProblemIo {
                    path: answer.to_owned(),
                    source,
                })?;

        let result = DefaultComparer::common()
            .compare(out, ans)
            .await
            .map_err(|source| PipelineError::Compare {
                output: output.to_owned(),
                answer: answer.to_owned(),
                source,
            })?;

        Ok(match result {
            CompareResult::Accepted => Verdict::Accepted,
            CompareResult::WrongAnswer => Verdict::WrongAnswer,
            CompareResult::PresentationError => Verdict::PresentationError,
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

    /// The CPU time limit. `None` means unlimited.
    pub time_limit: Option<Duration>,

    /// The total elapsed-time limit. `None` means unlimited.
    pub elapsed_limit: Option<Duration>,

    /// The address-space limit, in bytes. `None` means unlimited.
    pub memory_limit: Option<NonZeroU64>,
}

/// Render the execute template of `def` and split it into a program and its
/// arguments.
fn execute_command(
    template: &str,
    def: &LanguageDef,
    option: &LanguageOption,
    paths: &RenderPaths<'_>,
) -> Result<(PathBuf, Vec<String>), PipelineError> {
    let command =
        render(template, def, option, paths).map_err(|message| PipelineError::Render {
            step: "execute",
            message,
        })?;

    let mut parts = shell_words::split(&command).map_err(|err| PipelineError::Render {
        step: "execute",
        message: err.to_string(),
    })?;

    if parts.is_empty() {
        return Err(PipelineError::Render {
            step: "execute",
            message: "the execute template renders to an empty command".to_owned(),
        });
    }

    let program = parts.remove(0);
    let program = resolve_program(&program, paths.tmp_dir)?;

    Ok((program, parts))
}

/// Resolve the program of an execute template against the run directory and
/// the `PATH`.
fn resolve_program(program: &str, workspace: &Path) -> Result<PathBuf, PipelineError> {
    let path = Path::new(program);

    if path.is_absolute() {
        if path.is_file() {
            return Ok(path.to_owned());
        }

        return Err(PipelineError::MissingRuntime {
            program: program.to_owned(),
        });
    }

    if program.contains('/') {
        let candidate = workspace.join(path);

        if candidate.is_file() {
            return Ok(candidate);
        }

        return Err(PipelineError::MissingRuntime {
            program: program.to_owned(),
        });
    }

    std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|dir| dir.join(program))
                .find(|candidate| is_executable(candidate))
        })
        .ok_or_else(|| PipelineError::MissingRuntime {
            program: program.to_owned(),
        })
}

/// Whether `path` is a regular file with an execute bit set.
fn is_executable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

/// Refuse a compile step that produced no artifact.
fn ensure_artifact(path: &Path) -> Result<(), PipelineError> {
    if path.is_file() {
        Ok(())
    } else {
        Err(PipelineError::MissingArtifact {
            path: path.to_owned(),
        })
    }
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
