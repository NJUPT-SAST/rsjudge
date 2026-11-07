// SPDX-License-Identifier: Apache-2.0

//! The problem a submission is judged against.
//!
//! A problem is a `.tar.zst` archive whose root holds `problem.toml` and the
//! resources that file names. [`Problem::load`] extracts the archive into a
//! private directory, resolves the case paths against that directory, divides
//! the scores the configuration leaves open and checks that every referenced
//! file exists.

use std::collections::HashSet;
use std::num::{NonZeroU32, NonZeroU64};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use tar::{Archive, EntryType};
use tempfile::TempDir;
use thiserror::Error;

use crate::judger::request::cases::{CasesConfig, JudgeType, ResourceLimits, TaskType};

/// A problem loaded from a problem archive.
#[derive(Debug)]
pub struct Problem {
    /// How the outputs are judged.
    judge: JudgeType,

    /// The resource limits every case runs under.
    limits: ProblemLimits,

    /// The cases and how they are scored.
    task: ProblemTask,

    /// The score a fully accepted submission earns.
    total_score: u32,

    /// The extracted archive, removed when the problem is dropped.
    _guard: Option<TempDir>,
}

/// The resource limits of a problem, in the units the pipeline uses.
#[derive(Debug, Clone, Copy)]
pub struct ProblemLimits {
    /// The CPU time limit. `None` means unlimited.
    pub time: Option<Duration>,

    /// The wall-clock limit. `None` means unlimited.
    pub elapsed: Option<Duration>,

    /// The address-space limit in bytes. `None` means unlimited.
    pub memory: Option<NonZeroU64>,
}

/// The case layout of a problem.
#[derive(Debug, Clone)]
pub enum ProblemTask {
    /// Every case is scored on its own.
    Simple {
        /// The cases of the problem.
        cases: Vec<ProblemCase>,
    },

    /// Every subtask is scored as a whole.
    Subtask {
        /// The subtasks of the problem.
        subtasks: Vec<ProblemSubtask>,
    },
}

/// One case of a problem with resolved paths and score.
#[derive(Debug, Clone)]
pub struct ProblemCase {
    /// The input file.
    pub input: PathBuf,

    /// The answer file.
    pub answer: PathBuf,

    /// The score this case earns when it passes.
    pub score: u32,
}

/// One subtask of a problem with resolved paths and score.
#[derive(Debug, Clone)]
pub struct ProblemSubtask {
    /// The cases of the subtask.
    pub cases: Vec<ProblemCase>,

    /// The score this subtask earns when every case passes.
    pub score: u32,
}

/// A problem that could not be loaded.
#[derive(Debug, Error)]
pub enum ProblemError {
    /// The problem configuration file cannot be read.
    #[error("cannot read the problem config at {path}: {source}")]
    Read {
        /// The path of the configuration file.
        path: PathBuf,

        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The problem configuration file cannot be parsed.
    #[error("cannot parse the problem config at {path}: {source}")]
    Parse {
        /// The path of the configuration file.
        path: PathBuf,

        /// The underlying TOML error.
        #[source]
        source: toml::de::Error,
    },

    /// The problem archive cannot be read or extracted.
    #[error("cannot read the problem archive at {path}: {source}")]
    Archive {
        /// The path of the archive.
        path: PathBuf,

        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The problem configuration is inconsistent.
    #[error("invalid problem: {0}")]
    Invalid(String),
}

impl Problem {
    /// Load a `.tar.zst` problem archive.
    ///
    /// The archive is extracted into a private temporary directory that
    /// [`Problem`] owns, and its root must hold `problem.toml` together with
    /// the resources the file names.
    ///
    /// # Errors
    ///
    /// Returns an error when the archive cannot be read or extracted, when
    /// `problem.toml` is missing or cannot be parsed, when a case file it
    /// names is missing, or when its scores do not add up.
    pub fn load(path: &Path) -> Result<Self, ProblemError> {
        let dir = tempfile::Builder::new()
            .prefix("rsjudge-problem-")
            .tempdir()
            .map_err(|source| ProblemError::Archive {
                path: path.to_owned(),
                source,
            })?;

        extract_archive(path, dir.path())?;

        let config_path = dir.path().join("problem.toml");

        if !config_path.is_file() {
            return Err(ProblemError::Invalid(
                "the archive has no problem.toml at its root".to_owned(),
            ));
        }

        let text = std::fs::read_to_string(&config_path).map_err(|source| ProblemError::Read {
            path: config_path.clone(),
            source,
        })?;
        let config = toml::from_str(&text).map_err(|source| ProblemError::Parse {
            path: config_path,
            source,
        })?;

        let mut problem = Self::from_config(dir.path(), config)?;
        problem._guard = Some(dir);

        Ok(problem)
    }

    /// Build a problem from an already parsed configuration.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`load`](Self::load) for an inconsistent
    /// configuration or a missing file.
    pub fn from_config(dir: &Path, config: CasesConfig) -> Result<Self, ProblemError> {
        let limits = limits_of(&config.resource_limits);
        validate_judge(dir, &config.judge)?;

        let total_score = config.score.get();
        let task = match config.task {
            TaskType::Simple { cases } => {
                if cases.is_empty() {
                    return Err(ProblemError::Invalid("the problem has no cases".to_owned()));
                }

                let scores = resolve_scores(
                    total_score,
                    cases.iter().map(|case| case.score).collect(),
                    "case",
                )?;
                let mut resolved = Vec::with_capacity(cases.len());

                for (case, score) in cases.iter().zip(scores) {
                    resolved.push(ProblemCase {
                        input: check_file(dir, &case.input, "input")?,
                        answer: check_file(dir, &case.answer, "answer")?,
                        score,
                    });
                }

                ProblemTask::Simple { cases: resolved }
            }

            TaskType::Subtask { subtasks } => {
                if subtasks.is_empty() {
                    return Err(ProblemError::Invalid(
                        "the problem has no subtasks".to_owned(),
                    ));
                }

                let scores = resolve_scores(
                    total_score,
                    subtasks.iter().map(|subtask| subtask.score).collect(),
                    "subtask",
                )?;
                let mut resolved = Vec::with_capacity(subtasks.len());

                for (subtask, score) in subtasks.iter().zip(scores) {
                    if subtask.cases.is_empty() {
                        return Err(ProblemError::Invalid("a subtask has no cases".to_owned()));
                    }

                    let mut cases = Vec::with_capacity(subtask.cases.len());

                    for case in &subtask.cases {
                        if case.score.is_some() {
                            return Err(ProblemError::Invalid(
                                "a case inside a subtask carries a score; the subtask owns the score"
                                    .to_owned(),
                            ));
                        }

                        cases.push(ProblemCase {
                            input: check_file(dir, &case.input, "input")?,
                            answer: check_file(dir, &case.answer, "answer")?,
                            score: 0,
                        });
                    }

                    resolved.push(ProblemSubtask { cases, score });
                }

                ProblemTask::Subtask { subtasks: resolved }
            }
        };

        Ok(Self {
            judge: config.judge,
            limits,
            task,
            total_score,
            _guard: None,
        })
    }

    /// How the outputs are judged.
    #[must_use]
    pub fn judge(&self) -> &JudgeType {
        &self.judge
    }

    /// The resource limits every case runs under.
    #[must_use]
    pub fn limits(&self) -> ProblemLimits {
        self.limits
    }

    /// The cases and how they are scored.
    #[must_use]
    pub fn task(&self) -> &ProblemTask {
        &self.task
    }

    /// The score a fully accepted submission earns.
    #[must_use]
    pub fn total_score(&self) -> u32 {
        self.total_score
    }
}

/// Convert the configuration limits into the units of the pipeline.
///
/// A time or memory limit of zero leaves that limit unbounded. The wall-clock
/// limit gives a CPU-limited run twice its CPU time to finish.
fn limits_of(limits: &ResourceLimits) -> ProblemLimits {
    let time = (limits.time > 0).then(|| Duration::from_millis(u64::from(limits.time)));
    let elapsed = time.map(|time| time.saturating_mul(2));
    let memory = (limits.memory > 0).then(|| {
        NonZeroU64::new(u64::from(limits.memory) << 20)
            .expect("a positive megabyte limit is nonzero")
    });

    ProblemLimits {
        time,
        elapsed,
        memory,
    }
}

/// Extract a `.tar.zst` archive into `dest`.
///
/// The archive may hold regular files and directories only, every entry path
/// stays inside `dest`, and no path appears twice.
fn extract_archive(path: &Path, dest: &Path) -> Result<(), ProblemError> {
    let file = std::fs::File::open(path).map_err(|source| archive_error(path, source))?;
    let decoder =
        zstd::stream::read::Decoder::new(file).map_err(|source| archive_error(path, source))?;
    let mut archive = Archive::new(decoder);
    let entries = archive
        .entries()
        .map_err(|source| archive_error(path, source))?;
    let mut seen = HashSet::new();

    for entry in entries {
        let mut entry = entry.map_err(|source| archive_error(path, source))?;
        let entry_path = entry
            .path()
            .map_err(|source| archive_error(path, source))?
            .into_owned();
        validate_entry_path(&entry_path)?;

        let normalized = entry_path
            .components()
            .filter(|component| !matches!(component, Component::CurDir))
            .collect::<PathBuf>();

        if !seen.insert(normalized) {
            return Err(ProblemError::Invalid(format!(
                "the archive contains `{}` more than once",
                entry_path.display()
            )));
        }

        if !matches!(
            entry.header().entry_type(),
            EntryType::Regular | EntryType::Directory
        ) {
            return Err(ProblemError::Invalid(format!(
                "the archive entry `{}` is not a regular file or directory",
                entry_path.display()
            )));
        }

        if !entry
            .unpack_in(dest)
            .map_err(|source| archive_error(path, source))?
        {
            return Err(ProblemError::Invalid(format!(
                "the archive entry `{}` escapes the problem root",
                entry_path.display()
            )));
        }
    }

    Ok(())
}

/// Reject an archive entry whose path leaves the problem root.
fn validate_entry_path(path: &Path) -> Result<(), ProblemError> {
    if path.is_absolute() {
        return Err(ProblemError::Invalid(format!(
            "the archive entry `{}` is absolute",
            path.display()
        )));
    }

    if path.components().next().is_none() {
        return Err(ProblemError::Invalid(
            "the archive contains an entry with an empty path".to_owned(),
        ));
    }

    for component in path.components() {
        if matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        ) {
            return Err(ProblemError::Invalid(format!(
                "the archive entry `{}` leaves the problem root",
                path.display()
            )));
        }
    }

    Ok(())
}

/// Build an archive error for `path`.
fn archive_error(path: &Path, source: std::io::Error) -> ProblemError {
    ProblemError::Archive {
        path: path.to_owned(),
        source,
    }
}

/// Check that the files a judge references exist.
fn validate_judge(dir: &Path, judge: &JudgeType) -> Result<(), ProblemError> {
    match judge {
        JudgeType::Classic => Ok(()),
        JudgeType::SpecialJudge { checker } => {
            check_file(dir, checker, "checker")?;
            Ok(())
        }
        JudgeType::Interactive { interactor } => {
            check_file(dir, interactor, "interactor")?;
            Ok(())
        }
    }
}

/// Resolve `path` against the problem directory and check that it is a file.
fn check_file(dir: &Path, path: &Path, what: &str) -> Result<PathBuf, ProblemError> {
    let resolved = if path.is_absolute() {
        path.to_owned()
    } else {
        dir.join(path)
    };

    if !resolved.is_file() {
        return Err(ProblemError::Invalid(format!(
            "the {what} file `{}` does not exist",
            resolved.display()
        )));
    }

    Ok(resolved)
}

/// Give every case or subtask its score.
///
/// When none of them names a score, the problem total is divided evenly and
/// the first entries take the remainder. When all of them name one, the
/// scores must add up to the total.
fn resolve_scores(
    total: u32,
    scores: Vec<Option<NonZeroU32>>,
    what: &str,
) -> Result<Vec<u32>, ProblemError> {
    let count = scores.len();

    if count == 0 {
        return Err(ProblemError::Invalid(format!("the problem has no {what}s")));
    }

    let named = scores.iter().flatten().count();

    if named == 0 {
        let count_u32 = u32::try_from(count)
            .map_err(|_| ProblemError::Invalid(format!("the problem has too many {what}s")))?;
        let base = total / count_u32;
        let extra = usize::try_from(total % count_u32).unwrap_or(0);

        return Ok((0..count)
            .map(|index| base + u32::from(index < extra))
            .collect());
    }

    if named != scores.len() {
        return Err(ProblemError::Invalid(format!(
            "some {what}s carry a score and others do not"
        )));
    }

    let sum: u32 = scores.iter().flatten().copied().map(NonZeroU32::get).sum();

    if sum != total {
        return Err(ProblemError::Invalid(format!(
            "the {what} scores add up to {sum}, but the problem total is {total}"
        )));
    }

    Ok(scores.into_iter().flatten().map(NonZeroU32::get).collect())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use tempfile::TempDir;

    use super::{Problem, ProblemError, ProblemTask, validate_entry_path};
    use crate::judger::request::cases::CasesConfig;

    fn touch(dir: &TempDir, name: &str) {
        fs::write(dir.path().join(name), "").expect("failed to write a case file");
    }

    fn load(dir: &TempDir) -> Result<Problem, ProblemError> {
        let text = fs::read_to_string(dir.path().join("problem.toml"))
            .expect("failed to read the problem config");
        let config: CasesConfig =
            toml::from_str(&text).expect("failed to parse the problem config");

        Problem::from_config(dir.path(), config)
    }

    fn problem(dir: &TempDir) -> Problem {
        load(dir).expect("failed to load the problem")
    }

    fn write_config(dir: &TempDir, config: &str) {
        fs::write(dir.path().join("problem.toml"), config).expect("failed to write the config");
    }

    fn simple_config(first_score: &str, second_score: &str) -> String {
        format!(
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
{first_score}

[[task.cases]]
input = "2.in"
answer = "2.ans"
{second_score}
"#
        )
    }

    fn write_archive_files(dir: &TempDir, files: &[(&str, &str)]) -> PathBuf {
        let path = dir.path().join("problem.tar.zst");
        let file = fs::File::create(&path).expect("failed to create the problem archive");
        let encoder =
            zstd::stream::write::Encoder::new(file, 0).expect("failed to start the compressor");
        let mut archive = tar::Builder::new(encoder);

        for (name, contents) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(u64::try_from(contents.len()).expect("the entry fits in a tar header"));
            header.set_mode(0o644);
            header.set_cksum();
            archive
                .append_data(&mut header, name, contents.as_bytes())
                .expect("failed to append an archive entry");
        }

        let encoder = archive.into_inner().expect("failed to finish the archive");
        encoder.finish().expect("failed to finish the compression");

        path
    }

    #[test]
    fn loads_a_problem_archive() {
        let dir = TempDir::new().unwrap();
        let config = simple_config("", "");
        let archive = write_archive_files(
            &dir,
            &[
                ("problem.toml", &config),
                ("1.in", ""),
                ("1.ans", ""),
                ("2.in", ""),
                ("2.ans", ""),
            ],
        );

        let problem = Problem::load(&archive).expect("failed to load the problem archive");

        assert_eq!(problem.total_score(), 100);
        let ProblemTask::Simple { cases } = problem.task() else {
            panic!("a simple task loaded as a subtask");
        };
        assert_eq!(cases.len(), 2);
    }

    #[test]
    fn rejects_an_archive_without_a_config() {
        let dir = TempDir::new().unwrap();
        let archive = write_archive_files(&dir, &[("1.in", ""), ("1.ans", "")]);

        let error = Problem::load(&archive).expect_err("an archive without a config loaded");
        assert!(
            error.to_string().contains("problem.toml"),
            "the error does not name the missing config: {error}"
        );
    }

    #[test]
    fn rejects_archive_paths_outside_the_root() {
        assert!(validate_entry_path(Path::new("../escape")).is_err());
        assert!(validate_entry_path(Path::new("/absolute")).is_err());
        assert!(validate_entry_path(Path::new("nested/../../escape")).is_err());
        assert!(validate_entry_path(Path::new("nested/1.in")).is_ok());
        assert!(validate_entry_path(Path::new("./1.in")).is_ok());
    }

    #[test]
    fn loads_simple_cases_and_divides_the_total_score() {
        let dir = TempDir::new().unwrap();
        write_config(&dir, &simple_config("", ""));
        for name in ["1.in", "1.ans", "2.in", "2.ans"] {
            touch(&dir, name);
        }

        let problem = problem(&dir);

        assert_eq!(problem.total_score(), 100);
        assert_eq!(problem.limits().time.unwrap().as_millis(), 1000);
        assert_eq!(problem.limits().elapsed.unwrap().as_millis(), 2000);
        assert_eq!(problem.limits().memory.unwrap().get(), 256 << 20);

        let ProblemTask::Simple { cases } = problem.task() else {
            panic!("a simple task loaded as a subtask");
        };
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].score, 50);
        assert_eq!(cases[1].score, 50);
        assert!(cases[0].input.ends_with("1.in"));
        assert!(cases[0].answer.ends_with("1.ans"));
    }

    #[test]
    fn loads_subtasks_with_their_own_scores() {
        let dir = TempDir::new().unwrap();
        write_config(
            &dir,
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
score = 40

[[task.subtasks.cases]]
input = "1.in"
answer = "1.ans"

[[task.subtasks]]
score = 60

[[task.subtasks.cases]]
input = "2.in"
answer = "2.ans"
"#,
        );
        for name in ["1.in", "1.ans", "2.in", "2.ans"] {
            touch(&dir, name);
        }

        let problem = problem(&dir);

        let ProblemTask::Subtask { subtasks } = problem.task() else {
            panic!("a subtask task loaded as simple");
        };
        assert_eq!(subtasks.len(), 2);
        assert_eq!(subtasks[0].score, 40);
        assert_eq!(subtasks[1].score, 60);
    }

    #[test]
    fn rejects_a_missing_case_file() {
        let dir = TempDir::new().unwrap();
        write_config(&dir, &simple_config("", ""));
        touch(&dir, "1.ans");
        touch(&dir, "2.in");
        touch(&dir, "2.ans");

        let error = load(&dir).expect_err("a problem with a missing input loaded");
        assert!(
            error.to_string().contains("1.in"),
            "the error does not name the missing file: {error}"
        );
    }

    #[test]
    fn rejects_scores_that_do_not_add_up() {
        let dir = TempDir::new().unwrap();
        write_config(&dir, &simple_config("score = 40", "score = 50"));
        for name in ["1.in", "1.ans", "2.in", "2.ans"] {
            touch(&dir, name);
        }

        let error = load(&dir).expect_err("a problem with mismatched scores loaded");
        assert!(
            error.to_string().contains("add up"),
            "the error does not mention the score total: {error}"
        );
    }

    #[test]
    fn rejects_a_mix_of_named_and_unnamed_case_scores() {
        let dir = TempDir::new().unwrap();
        write_config(&dir, &simple_config("score = 50", ""));
        for name in ["1.in", "1.ans", "2.in", "2.ans"] {
            touch(&dir, name);
        }

        let error = load(&dir).expect_err("a mixed score problem loaded");
        assert!(
            error.to_string().contains("some cases"),
            "the error does not mention the mixed scores: {error}"
        );
    }

    #[test]
    fn rejects_a_case_score_inside_a_subtask() {
        let dir = TempDir::new().unwrap();
        write_config(
            &dir,
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
score = 10
"#,
        );
        touch(&dir, "1.in");
        touch(&dir, "1.ans");

        let error = load(&dir).expect_err("a subtask case with a score loaded");
        assert!(
            error.to_string().contains("the subtask owns the score"),
            "the error does not explain the subtask score: {error}"
        );
    }

    #[test]
    fn zero_limits_mean_unlimited() {
        let dir = TempDir::new().unwrap();
        write_config(
            &dir,
            r#"
score = 100

[judge]
judgeType = "classic"

[resourceLimits]
time = 0
memory = 0

[task]
taskType = "simple"

[[task.cases]]
input = "1.in"
answer = "1.ans"
"#,
        );
        touch(&dir, "1.in");
        touch(&dir, "1.ans");

        let problem = problem(&dir);

        assert!(problem.limits().time.is_none());
        assert!(problem.limits().elapsed.is_none());
        assert!(problem.limits().memory.is_none());
    }

    #[test]
    fn rejects_a_missing_checker() {
        let dir = TempDir::new().unwrap();
        write_config(
            &dir,
            r#"
score = 100

[judge]
judgeType = "special-judge"
checker = "checker"

[resourceLimits]
time = 1000
memory = 256

[task]
taskType = "simple"

[[task.cases]]
input = "1.in"
answer = "1.ans"
"#,
        );
        touch(&dir, "1.in");
        touch(&dir, "1.ans");

        let error = load(&dir).expect_err("a missing checker loaded");
        assert!(
            error.to_string().contains("checker"),
            "the error does not mention the checker: {error}"
        );
    }
}
