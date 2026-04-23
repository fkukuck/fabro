#[test]
fn generate_params_requires_client() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/generate_params_requires_client.rs");
}
