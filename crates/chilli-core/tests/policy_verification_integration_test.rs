use async_trait::async_trait;
use chilli_core::agent::CapabilityAgentRunner;
use chilli_core::verifier::{
    RawCheckOutput, VerificationExecutor, VerificationInfrastructureError, VerificationResult,
};
use chilli_model::capabilities::ModelCapabilityProfile;
use chilli_orchestrator::complexity::{CodeComplexity, ReasoningComplexity, TaskComplexity};
use std::path::Path;

struct MockFailingVerifier;

#[async_trait]
impl VerificationExecutor for MockFailingVerifier {
    async fn execute(
        &self,
        _workspace_root: &Path,
    ) -> Result<RawCheckOutput, VerificationInfrastructureError> {
        Ok(RawCheckOutput {
            exit_code: 101,
            stdout: "".to_string(),
            stderr: "error[E0425]: cannot find value `foo` in this scope\n --> src/lib.rs:5:1"
                .to_string(),
        })
    }
}

#[tokio::test]
async fn test_agent_runner_executes_mutation_verifies_and_captures_diagnostic() {
    let mut profile = ModelCapabilityProfile::new("mock-provider", "model-small", 4096);
    let complexity = TaskComplexity {
        code: CodeComplexity {
            symbol_count: 5,
            files_count: 1,
            dependency_depth: 1,
            blast_radius_files: 1,
        },
        reasoning: ReasoningComplexity {
            ambiguity_score: 0.2,
            architectural_change: false,
            concurrency_risk: false,
            invariant_count: 1,
            verification_difficulty: 0.2,
        },
    };

    let mut runner = CapabilityAgentRunner::new(Box::new(MockFailingVerifier));
    let result = runner
        .execute_step(Path::new("."), &mut profile, &complexity)
        .await;

    match result {
        VerificationResult::Failed(slice) => {
            assert!(slice.evidence.iter().any(|e| e.contains("E0425")));
            // Ensure observation was recorded in capability profile
            assert_eq!(profile.observations.len(), 1);
        }
        VerificationResult::Passed => panic!("Expected verification failure slice"),
    }
}
