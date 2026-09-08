use chilli_core::engine::AgentEngine;
use chilli_orchestrator::types::{VerificationSpec, VerificationType};
use tempfile::tempdir;

#[tokio::test]
async fn test_verification_failure_triggers_recovery_attempt() {
    let dir = tempdir().unwrap();
    let mut engine = AgentEngine::new(dir.path()).unwrap();

    let spec = VerificationSpec {
        checks: vec![VerificationType::FileExists {
            path: "required.txt".to_string(),
        }],
    };
    engine.set_verification_spec(spec);

    let failures = engine.check_verification().unwrap();
    assert!(failures.is_some());
    let reasons = failures.unwrap();
    assert!(reasons[0].contains("required.txt"));
}
