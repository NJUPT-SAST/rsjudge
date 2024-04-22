// SPDX-License-Identifier: Apache-2.0

pub trait Judger {
    type Result;
    fn accept_languages(&self) -> Vec<String>;
    fn judge(&self, lang: &str, code: &str, input: &str) -> Self::Result;
}
