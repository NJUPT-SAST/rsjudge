// SPDX-License-Identifier: Apache-2.0

//! The crate against the reference fixtures of the `schemas/test-cases-schema`
//! submodule.
//!
//! The submodule generates `out/*.json` and `out/*.toml` from its TypeScript
//! definitions with `index.ts`, so those files are the contract both
//! implementations have to satisfy. Deserializing them here is what stops the
//! Rust types from drifting away from the TypeScript ones.

use std::path::{Path, PathBuf};

use rsjudge_test_cases_schema::{CasesConfig, JudgeType, TaskType};

/// The fixture directory of the submodule.
fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../schemas/test-cases-schema/out")
        .canonicalize()
        .unwrap_or_else(|source| {
            panic!(
                "the fixture directory of the `schemas/test-cases-schema` submodule is missing \
                 ({source}); run `git submodule update --init --recursive` first"
            )
        })
}

/// The fixtures and the judge and task types each one exercises.
const FIXTURES: &[(&str, &str, &str)] = &[
    ("classic-simple", "classic", "simple"),
    ("classic-subtasks", "classic", "subtask"),
    ("special-judge-subtasks", "special-judge", "subtask"),
    ("interactive-simple", "interactive", "simple"),
];

/// The judge type of a configuration, as the fixture names it.
fn judge_name(judge: &JudgeType) -> &'static str {
    match judge {
        JudgeType::Classic => "classic",
        JudgeType::SpecialJudge { .. } => "special-judge",
        JudgeType::Interactive { .. } => "interactive",
    }
}

/// The task type of a configuration, as the fixture names it.
fn task_name(task: &TaskType) -> &'static str {
    match task {
        TaskType::Simple { .. } => "simple",
        TaskType::Subtask { .. } => "subtask",
    }
}

#[test]
fn every_toml_fixture_deserializes() {
    let dir = fixtures();

    for (name, judge, task) in FIXTURES {
        let path = dir.join(format!("{name}.toml"));
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|source| panic!("cannot read {}: {source}", path.display()));
        let config: CasesConfig = toml::from_str(&text)
            .unwrap_or_else(|source| panic!("cannot parse {}: {source}", path.display()));

        assert_eq!(judge_name(&config.judge), *judge, "{name}.toml judge type");
        assert_eq!(task_name(&config.task), *task, "{name}.toml task type");
    }
}

#[test]
fn every_json_fixture_deserializes() {
    let dir = fixtures();

    for (name, judge, task) in FIXTURES {
        let path = dir.join(format!("{name}.json"));
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|source| panic!("cannot read {}: {source}", path.display()));
        let config: CasesConfig = serde_json::from_str(&text)
            .unwrap_or_else(|source| panic!("cannot parse {}: {source}", path.display()));

        assert_eq!(judge_name(&config.judge), *judge, "{name}.json judge type");
        assert_eq!(task_name(&config.task), *task, "{name}.json task type");
    }
}

#[test]
fn json_and_toml_fixtures_agree() {
    let dir = fixtures();

    for (name, _, _) in FIXTURES {
        let toml_path = dir.join(format!("{name}.toml"));
        let json_path = dir.join(format!("{name}.json"));

        let toml_text = std::fs::read_to_string(&toml_path)
            .unwrap_or_else(|source| panic!("cannot read {}: {source}", toml_path.display()));
        let json_text = std::fs::read_to_string(&json_path)
            .unwrap_or_else(|source| panic!("cannot read {}: {source}", json_path.display()));

        let from_toml: CasesConfig = toml::from_str(&toml_text)
            .unwrap_or_else(|source| panic!("cannot parse {}: {source}", toml_path.display()));
        let from_json: CasesConfig = serde_json::from_str(&json_text)
            .unwrap_or_else(|source| panic!("cannot parse {}: {source}", json_path.display()));

        // Both fixtures describe the same problem, so a value read from one has
        // to serialize to the same JSON as the value read from the other.
        assert_eq!(
            serde_json::to_value(&from_toml).expect("the TOML fixture serializes"),
            serde_json::to_value(&from_json).expect("the JSON fixture serializes"),
            "{name}: the TOML and JSON fixtures disagree"
        );
    }
}
