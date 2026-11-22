// SPDX-License-Identifier: Apache-2.0

//! The problem package format and the `problem.toml` configuration schema.
//!
//! A problem is a `.tar.zst` archive whose root holds `problem.toml` and the
//! resources that file names. The types here are the serde implementation the
//! judging pipeline parses with; the reference fixtures the tests read come
//! from the `schemas/test-cases-schema` submodule, which keeps the TypeScript
//! definitions and the shared examples.

#![warn(missing_docs)]

use std::num::NonZeroU32;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The root of a `problem.toml` file.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CasesConfig {
    /// The total score of the problem.
    pub score: NonZeroU32,

    /// How the submission's output is judged.
    pub judge: JudgeType,

    /// The resource limits every case runs under.
    pub resource_limits: ResourceLimits,

    /// The cases and how they are scored.
    pub task: TaskType,
}

/// How the output of a submission is judged.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "judgeType")]
pub enum JudgeType {
    /// Compare the output with the answer file.
    Classic,

    /// Run a trusted checker on the input, the answer and the output.
    SpecialJudge {
        /// The checker program, relative to the problem root.
        checker: PathBuf,
    },

    /// Run a trusted interactor that talks to the submission.
    Interactive {
        /// The interactor program, relative to the problem root.
        interactor: PathBuf,
    },
}

/// The resource limits of a problem.
#[derive(Debug, Deserialize, Serialize)]
pub struct ResourceLimits {
    /// The CPU time limit, in milliseconds. Zero means unlimited.
    pub time: u32,

    /// The address-space limit, in megabytes. Zero means unlimited.
    pub memory: u32,
}

/// The task layout of a problem.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "taskType")]
pub enum TaskType {
    /// Every case is scored on its own.
    Simple {
        /// The cases of the problem.
        cases: Vec<Case>,
    },

    /// Every subtask is scored as a whole.
    Subtask {
        /// The subtasks of the problem.
        subtasks: Vec<Subtask>,
    },
}

/// One case of a problem.
#[derive(Debug, Deserialize, Serialize)]
pub struct Case {
    /// The input file, relative to the problem root.
    pub input: PathBuf,

    /// The answer file, relative to the problem root.
    pub answer: PathBuf,

    /// The score of this case. Simple tasks may omit it, and then the
    /// problem's total score is divided evenly among the cases.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<NonZeroU32>,
}

/// One subtask of a problem.
#[derive(Debug, Deserialize, Serialize)]
pub struct Subtask {
    /// The cases of the subtask.
    pub cases: Vec<Case>,

    /// The score of this subtask. Subtasks may omit it, and then the
    /// problem's total score is divided evenly among the subtasks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<NonZeroU32>,
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use serde_json::json;

    use crate::{Case, CasesConfig, JudgeType, ResourceLimits, TaskType};

    fn non_zero(value: u32) -> NonZeroU32 {
        debug_assert_ne!(value, 0);
        NonZeroU32::new(value).unwrap()
    }

    #[test]
    fn serialize_test() {
        serde_json::to_string_pretty(&CasesConfig {
            score: non_zero(100),
            judge: JudgeType::Classic,
            resource_limits: ResourceLimits {
                time: 1000,
                memory: 512,
            },
            task: TaskType::Simple {
                cases: vec![
                    Case {
                        input: "1.in".into(),
                        answer: "1.ans".into(),
                        score: None,
                    },
                    Case {
                        input: "2.in".into(),
                        answer: "2.ans".into(),
                        score: NonZeroU32::new(60),
                    },
                ],
            },
        })
        .unwrap();
    }

    #[test]
    fn deserialize_test() {
        serde_json::from_value::<CasesConfig>(json!({
          "score": 100,
          "judge": {
            "judgeType": "classic"
          },
          "resourceLimits": {
            "time": 1000,
            "memory": 256
          },
          "task": {
            "taskType": "simple",
            "cases": [
              {
                "input": "1.in",
                "answer": "1.ans"
              },
              {
                "input": "2.in",
                "answer": "2.ans",
                "score": 60
              }
            ]
          }
        }))
        .unwrap();

        serde_json::from_value::<CasesConfig>(json!({
          "score": 100,
          "judge": {
            "judgeType": "special-judge",
            "checker": "checker.cpp"
          },
          "resourceLimits": {
            "time": 1000,
            "memory": 256
          },
          "task": {
            "taskType": "subtask",
            "subtasks": [
              {
                "cases": [
                  {
                    "input": "1.in",
                    "answer": "1.ans"
                  },
                  {
                    "input": "2.in",
                    "answer": "2.ans"
                  }
                ],
                "score": 40
              },
              {
                "cases": [
                  {
                    "input": "3.in",
                    "answer": "3.ans"
                  },
                  {
                    "input": "4.in",
                    "answer": "4.ans"
                  }
                ],
                "score": 60
              }
            ]
          }
        }))
        .unwrap();
    }
}
