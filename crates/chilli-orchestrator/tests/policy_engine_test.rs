use chilli_model::capabilities::{CapabilityAssessment, CapabilityTier};
use chilli_orchestrator::complexity::{CodeComplexity, ReasoningComplexity, TaskComplexity};
use chilli_orchestrator::policy::{ExecutionPolicyEngine, ExecutionStrategy, TaskExecutionState};

#[test]
fn test_frontier_with_high_reasoning_complexity_and_failures_gets_guided_strategy() {
    let assessment = CapabilityAssessment {
        current_tier: CapabilityTier::Frontier,
        confidence: 1.0,
        is_cold_start: false,
        promotion_threshold: 0.75,
        demotion_threshold: 0.45,
        evidence_window: 3,
    };
    let complexity = TaskComplexity {
        code: CodeComplexity {
            symbol_count: 50,
            files_count: 8,
            dependency_depth: 4,
            blast_radius_files: 10,
        },
        reasoning: ReasoningComplexity {
            ambiguity_score: 0.9,
            architectural_change: true,
            concurrency_risk: true,
            invariant_count: 5,
            verification_difficulty: 0.8,
        },
    };
    let state = TaskExecutionState {
        attempt_count: 2,
        verification_failures: 2,
        recent_tool_failures: 1,
        repeated_failure_count: 2,
    };

    let strategy = ExecutionPolicyEngine::select_strategy(&assessment, &complexity, &state);
    // Frontier tier gets demoted to Guided strategy due to high architectural risk + repeated verification failures
    assert!(matches!(strategy, ExecutionStrategy::Guided { .. }));
}
