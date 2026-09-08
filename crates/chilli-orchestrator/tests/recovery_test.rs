use chilli_orchestrator::recovery::{FailureCategory, RecoveryAction, RecoveryEngine};

#[test]
fn test_classify_compile_error() {
    let stderr = "error[E0425]: cannot find value `foo` in this scope";
    let cat = RecoveryEngine::classify_error(stderr);
    assert_eq!(cat, FailureCategory::SyntacticCompile);

    let action = RecoveryEngine::determine_action(&cat, 1);
    assert_eq!(action, RecoveryAction::RetryWorkerWithErrorPrompt);
}

#[test]
fn test_classify_context_overflow() {
    let stderr = "maximum context length exceeded: 128000 tokens";
    let cat = RecoveryEngine::classify_error(stderr);
    assert_eq!(cat, FailureCategory::ContextOverflow);

    let action = RecoveryEngine::determine_action(&cat, 1);
    assert_eq!(action, RecoveryAction::CompactContextAndRetry);
}
