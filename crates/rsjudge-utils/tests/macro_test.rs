use trybuild::TestCases;

#[test]
fn tests() {
    let test_cases = TestCases::new();
    test_cases.compile_fail("tests/logging_on_non_result.test");
}
