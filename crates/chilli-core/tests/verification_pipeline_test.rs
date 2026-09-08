use async_trait::async_trait;
use chilli_core::verifier::{
    FailureCategory, RawCheckOutput, VerificationExecutor, VerificationGate,
    VerificationInfrastructureError, VerificationResult,
};
use std::path::Path;

struct MockAsyncVerifier {
    output: RawCheckOutput,
}

#[async_trait]
impl VerificationExecutor for MockAsyncVerifier {
    async fn execute(
        &self,
        _workspace_root: &Path,
    ) -> Result<RawCheckOutput, VerificationInfrastructureError> {
        Ok(self.output.clone())
    }
}

#[tokio::test]
async fn test_async_verification_pipeline_compresses_failures_into_diagnostic_slice() {
    let mock_output = RawCheckOutput {
        exit_code: 1,
        stdout: String::new(),
        stderr: "error[E0425]: cannot find value `foo` in this scope\n  --> src/main.rs:10:5"
            .to_string(),
    };
    let verifier = MockAsyncVerifier {
        output: mock_output,
    };

    let gate = VerificationGate::new(vec![Box::new(verifier)]);
    let result = gate.run_verification(Path::new(".")).await.unwrap();

    match result {
        VerificationResult::Passed => panic!("Expected verification to fail"),
        VerificationResult::Failed(diagnostic_slice) => {
            assert_eq!(diagnostic_slice.category, FailureCategory::UnresolvedSymbol);
            assert!(diagnostic_slice
                .evidence
                .iter()
                .any(|e| e.contains("E0425")));
            assert!(diagnostic_slice.context_budget_bytes <= 1024);
        }
    }
}
