// SPDX-License-Identifier: Apache-2.0

use trybuild::TestCases;

#[test]
fn tests() {
    let test_cases = TestCases::new();
    test_cases.compile_fail("tests/cases/logging_on_non_result.rs");
}
