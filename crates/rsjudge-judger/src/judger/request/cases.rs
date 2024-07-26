use std::{num::NonZeroU32, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CasesConfig {
    score: NonZeroU32,
    judge: JudgeType,
    resource_limits: ResourceLimits,
    task: TaskType,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "judgeType")]
pub enum JudgeType {
    Classic,
    SpecialJudge { checker: PathBuf },
    Interactive { interactor: PathBuf },
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ResourceLimits {
    time: u32,
    memory: u32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "taskType")]
pub enum TaskType {
    Simple { cases: Vec<Case> },
    Subtask { subtasks: Vec<Subtask> },
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Case {
    input: PathBuf,
    answer: PathBuf,

    #[serde(skip_serializing_if = "Option::is_none")]
    score: Option<NonZeroU32>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Subtask {
    cases: Vec<Case>,

    #[serde(skip_serializing_if = "Option::is_none")]
    score: Option<NonZeroU32>,
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use serde_json::json;

    use super::{Case, CasesConfig, JudgeType, ResourceLimits, TaskType};

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
