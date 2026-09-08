use chilli_orchestrator::pipeline::OrchestratorPipeline;
use chilli_orchestrator::types::{AgentRole, PlannerTaskSpec, TaskPlan};

#[tokio::test]
async fn test_sequential_pipeline_execution() {
    let plan = TaskPlan {
        plan_id: "plan-mock".to_string(),
        goal: "Pass mock verification".to_string(),
        tasks: vec![
            PlannerTaskSpec {
                id: "planner-task".to_string(),
                description: "Plan tasks".to_string(),
                role: AgentRole::Planner,
                depends_on: vec![],
                expected_artifacts: vec![],
            },
            PlannerTaskSpec {
                id: "worker-task".to_string(),
                description: "Write code".to_string(),
                role: AgentRole::Worker,
                depends_on: vec!["planner-task".to_string()],
                expected_artifacts: vec![],
            },
        ],
    };

    let pipeline = OrchestratorPipeline::new();
    let result = pipeline.run_mock_sequential(plan).await.unwrap();

    assert!(result.success);
    assert_eq!(result.verified_steps, 2);
}

#[test]
fn test_pipeline_verify_step() {
    let res = OrchestratorPipeline::verify_step(true, true, true, "OK");
    assert!(res.passed);

    let res_fail =
        OrchestratorPipeline::verify_step(false, true, true, "error[E0425]: cannot find value");
    assert!(!res_fail.passed);
}

#[test]
fn test_pipeline_handle_verification_failure() {
    use chilli_orchestrator::recovery::{FailureCategory, RecoveryAction};

    let (cat, act) =
        OrchestratorPipeline::handle_verification_failure("error[E0308]: mismatched types", 1);
    assert_eq!(cat, FailureCategory::SyntacticCompile);
    assert_eq!(act, RecoveryAction::RetryWorkerWithErrorPrompt);

    let (cat_esc, act_esc) =
        OrchestratorPipeline::handle_verification_failure("error[E0308]: mismatched types", 3);
    assert_eq!(cat_esc, FailureCategory::SyntacticCompile);
    assert_eq!(act_esc, RecoveryAction::EscalateModelReasoningTier);
}

#[tokio::test]
async fn test_autonomous_loop_execution() {
    let plan = TaskPlan {
        plan_id: "plan-auto-loop".to_string(),
        goal: "Autonomous workflow execution".to_string(),
        tasks: vec![
            PlannerTaskSpec {
                id: "plan-step".to_string(),
                description: "Initial plan".to_string(),
                role: AgentRole::Planner,
                depends_on: vec![],
                expected_artifacts: vec!["plan.md".to_string()],
            },
            PlannerTaskSpec {
                id: "worker-1".to_string(),
                description: "Worker module 1".to_string(),
                role: AgentRole::Worker,
                depends_on: vec!["plan-step".to_string()],
                expected_artifacts: vec!["src/mod1.rs".to_string()],
            },
            PlannerTaskSpec {
                id: "worker-2".to_string(),
                description: "Worker module 2".to_string(),
                role: AgentRole::Worker,
                depends_on: vec!["plan-step".to_string()],
                expected_artifacts: vec!["src/mod2.rs".to_string()],
            },
            PlannerTaskSpec {
                id: "verifier-step".to_string(),
                description: "Final verifier".to_string(),
                role: AgentRole::Verifier,
                depends_on: vec!["worker-1".to_string(), "worker-2".to_string()],
                expected_artifacts: vec!["test.log".to_string()],
            },
        ],
    };

    let pipeline = OrchestratorPipeline::new();
    let result = pipeline.run_autonomous_loop(plan, 3).await.unwrap();

    assert!(result.success);
    assert_eq!(result.verified_steps, 4);
    assert_eq!(result.failed_steps, 0);
}
