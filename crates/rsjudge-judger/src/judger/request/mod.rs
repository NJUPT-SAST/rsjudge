use bytes::Bytes;

use crate::judger::request::{cases::CasesConfig, source::Source};

mod source;

mod cases;

pub struct JudgeRequest {
    source: Source,
    judge_type: JudgeType,
}

impl JudgeRequest {
    pub fn new(source: Source, judge_type: JudgeType) -> Self {
        Self { source, judge_type }
    }

    pub fn source(&self) -> &Source {
        &self.source
    }

    pub fn judge_type(&self) -> &JudgeType {
        &self.judge_type
    }
}

pub enum JudgeType {
    SelfTest { input: Bytes },
    Submit { cases: CasesConfig },
}
