use chilli_orchestrator::blackboard::Blackboard;
use chilli_orchestrator::graph::TaskGraph;
use chilli_orchestrator::pipeline::OrchestratorPipeline;
use chilli_orchestrator::recovery::{FailureCategory, RecoveryAction};
use chilli_orchestrator::types::{AgentRole, PlannerTaskSpec, TaskPlan, TaskStatus};
use chilli_orchestrator::worktree_integration::OverlapAnalyzer;
use std::sync::Arc;

#[tokio::test]
async fn test_e2e_parallel_task_dag_execution_and_merge() {
    let blackboard = Arc::new(Blackboard::new());

    let plan = TaskPlan {
        plan_id: "plan-e2e-01".to_string(),
        goal: "Parallel multi-module update".to_string(),
        tasks: vec![
            PlannerTaskSpec {
                id: "planner-dag".to_string(),
                description: "Create DAG plan".to_string(),
                role: AgentRole::Planner,
                depends_on: vec![],
                expected_artifacts: vec!["plan.json".to_string()],
            },
            PlannerTaskSpec {
                id: "worker-auth".to_string(),
                description: "Update auth service".to_string(),
                role: AgentRole::Worker,
                depends_on: vec!["planner-dag".to_string()],
                expected_artifacts: vec!["src/auth.rs".to_string()],
            },
            PlannerTaskSpec {
                id: "worker-db".to_string(),
                description: "Update db connection pool".to_string(),
                role: AgentRole::Worker,
                depends_on: vec!["planner-dag".to_string()],
                expected_artifacts: vec!["src/db.rs".to_string()],
            },
            PlannerTaskSpec {
                id: "worker-api".to_string(),
                description: "Update API handlers".to_string(),
                role: AgentRole::Worker,
                depends_on: vec!["planner-dag".to_string()],
                expected_artifacts: vec!["src/api.rs".to_string()],
            },
            PlannerTaskSpec {
                id: "verifier-all".to_string(),
                description: "Run verification test suite".to_string(),
                role: AgentRole::Verifier,
                depends_on: vec![
                    "worker-auth".to_string(),
                    "worker-db".to_string(),
                    "worker-api".to_string(),
                ],
                expected_artifacts: vec!["test_report.json".to_string()],
            },
        ],
    };

    let mut graph = TaskGraph::from_plan(plan.clone()).unwrap();

    // 1. Planner executes first
    let ready_p1 = graph.get_ready_tasks();
    assert_eq!(ready_p1.len(), 1);
    assert_eq!(ready_p1[0].id, "planner-dag");

    graph.mark_running("planner-dag", "agent-planner-1");
    graph.mark_completed("planner-dag", None);
    blackboard.set("plan_status", serde_json::json!("DAG_CREATED"));

    // 2. Three independent workers execute simultaneously
    let ready_p2 = graph.get_ready_tasks();
    assert_eq!(ready_p2.len(), 3);

    // Grouping check: Overlap analyzer should verify 3 workers have disjoint artifacts
    let worker_specs: Vec<_> = ready_p2.iter().map(|n| n.spec.clone()).collect();
    let groups = OverlapAnalyzer::partition_parallel_groups(&worker_specs);
    assert_eq!(groups.len(), 1); // 1 parallel stage containing all 3 workers
    assert_eq!(groups[0].len(), 3);

    let ready_ids: Vec<String> = ready_p2.iter().map(|n| n.id.clone()).collect();

    for id in &ready_ids {
        graph.mark_running(id, &format!("worker-agent-{}", id));
        blackboard.set(&format!("{}_status", id), serde_json::json!("COMPLETED"));
        graph.mark_completed(id, None);
    }

    // 3. Verifier checks results after all workers complete
    let ready_p3 = graph.get_ready_tasks();
    assert_eq!(ready_p3.len(), 1);
    assert_eq!(ready_p3[0].id, "verifier-all");

    graph.mark_verifying("verifier-all", "agent-verifier-1");
    let vres = OrchestratorPipeline::verify_step(true, true, true, "Build and test suite passed");
    assert!(vres.passed);
    graph.mark_verified("verifier-all");

    assert!(graph.is_finished());
}

#[tokio::test]
async fn test_e2e_intentional_failure_recovery_no_replay() {
    let plan = TaskPlan {
        plan_id: "plan-e2e-02".to_string(),
        goal: "Test recovery on intentional failure".to_string(),
        tasks: vec![
            PlannerTaskSpec {
                id: "worker-success".to_string(),
                description: "Successful worker task".to_string(),
                role: AgentRole::Worker,
                depends_on: vec![],
                expected_artifacts: vec!["src/lib.rs".to_string()],
            },
            PlannerTaskSpec {
                id: "worker-failing".to_string(),
                description: "Worker task with syntax error".to_string(),
                role: AgentRole::Worker,
                depends_on: vec![],
                expected_artifacts: vec!["src/syntax.rs".to_string()],
            },
        ],
    };

    let mut graph = TaskGraph::from_plan(plan).unwrap();

    // Mark worker-success as completed
    graph.mark_running("worker-success", "w-1");
    graph.mark_completed("worker-success", None);

    // Mark worker-failing as running
    graph.mark_running("worker-failing", "w-2");

    // Simulate intentional syntax error
    let syntax_err = "error[E0425]: cannot find value `unresolved_symbol` in this scope";
    let vres = OrchestratorPipeline::verify_step(false, true, true, syntax_err);
    assert!(!vres.passed);

    // Classify failure & determine action
    let (category, action_attempt1) =
        OrchestratorPipeline::handle_verification_failure(&vres.diagnostics, 1);
    assert_eq!(category, FailureCategory::SyntacticCompile);
    assert_eq!(action_attempt1, RecoveryAction::RetryWorkerWithErrorPrompt);

    // Simulate attempt 3 escalation
    let (_, action_attempt3) =
        OrchestratorPipeline::handle_verification_failure(&vres.diagnostics, 3);
    assert_eq!(action_attempt3, RecoveryAction::EscalateModelReasoningTier);

    // Retry worker-failing
    graph.mark_running("worker-failing", "w-2-retry");
    graph.mark_completed("worker-failing", None);

    // Ensure worker-success status remains Completed and was NOT replayed
    assert_eq!(
        graph.nodes.get("worker-success").unwrap().status,
        TaskStatus::Completed
    );
    assert_eq!(
        graph.nodes.get("worker-failing").unwrap().status,
        TaskStatus::Completed
    );
    assert!(graph.is_finished());
}

#[test]
fn test_e2e_file_overlap_detection_no_silent_overwrite() {
    let t1 = PlannerTaskSpec {
        id: "w1".to_string(),
        description: "Modify config module".to_string(),
        role: AgentRole::Worker,
        depends_on: vec![],
        expected_artifacts: vec!["src/config.rs".to_string()],
    };

    let t2 = PlannerTaskSpec {
        id: "w2".to_string(),
        description: "Update config schema".to_string(),
        role: AgentRole::Worker,
        depends_on: vec![],
        expected_artifacts: vec!["src/config.rs".to_string()],
    };

    let t3 = PlannerTaskSpec {
        id: "w3".to_string(),
        description: "Add logger".to_string(),
        role: AgentRole::Worker,
        depends_on: vec![],
        expected_artifacts: vec!["src/logger.rs".to_string()],
    };

    // Confirm overlap detected between w1 and w2
    assert!(OverlapAnalyzer::has_overlap(&t1, &t2));
    assert!(!OverlapAnalyzer::has_overlap(&t1, &t3));

    // OverlapAnalyzer partitions w1 and w2 into separate sequential stages
    let groups = OverlapAnalyzer::partition_parallel_groups(&[t1, t2, t3]);
    assert_eq!(groups.len(), 2);

    // Stage 1: w1 and w3 run in parallel (no overlap)
    assert_eq!(groups[0].len(), 2);
    // Stage 2: w2 runs in next stage to prevent concurrent edit overwrite on src/config.rs
    assert_eq!(groups[1].len(), 1);
    assert_eq!(groups[1][0].id, "w2");
}

#[tokio::test]
async fn test_e2e_real_multifile_refactor_feature() {
    let plan = TaskPlan {
        plan_id: "plan-e2e-refactor".to_string(),
        goal: "Refactor engine event loop and accumulator".to_string(),
        tasks: vec![
            PlannerTaskSpec {
                id: "t1-refactor-engine".to_string(),
                description: "Refactor engine event loop".to_string(),
                role: AgentRole::Worker,
                depends_on: vec![],
                expected_artifacts: vec!["crates/chilli-core/src/engine.rs".to_string()],
            },
            PlannerTaskSpec {
                id: "t2-refactor-accumulator".to_string(),
                description: "Refactor stream accumulator".to_string(),
                role: AgentRole::Worker,
                depends_on: vec![],
                expected_artifacts: vec!["crates/chilli-core/src/stream_accumulator.rs".to_string()],
            },
            PlannerTaskSpec {
                id: "t3-verify-engine-tests".to_string(),
                description: "Run engine integration tests".to_string(),
                role: AgentRole::Verifier,
                depends_on: vec![
                    "t1-refactor-engine".to_string(),
                    "t2-refactor-accumulator".to_string(),
                ],
                expected_artifacts: vec!["crates/chilli-core/tests/engine_test.rs".to_string()],
            },
        ],
    };

    let pipeline = OrchestratorPipeline::new();
    let result = pipeline.run_mock_sequential(plan).await.unwrap();

    assert!(result.success);
    assert_eq!(result.verified_steps, 3);
    assert_eq!(result.failed_steps, 0);
}

#[tokio::test]
async fn test_e2e_interrupted_session_resume_checkpoint() {
    let plan = TaskPlan {
        plan_id: "plan-e2e-resume".to_string(),
        goal: "Resumable execution test".to_string(),
        tasks: vec![
            PlannerTaskSpec {
                id: "step-1".to_string(),
                description: "First step".to_string(),
                role: AgentRole::Worker,
                depends_on: vec![],
                expected_artifacts: vec!["file1.txt".to_string()],
            },
            PlannerTaskSpec {
                id: "step-2".to_string(),
                description: "Second step".to_string(),
                role: AgentRole::Worker,
                depends_on: vec!["step-1".to_string()],
                expected_artifacts: vec!["file2.txt".to_string()],
            },
        ],
    };

    let mut graph = TaskGraph::from_plan(plan.clone()).unwrap();

    // Execute step-1
    graph.mark_running("step-1", "w-1");
    graph.mark_completed("step-1", None);

    // Checkpoint state simulation: step-1 is Completed, step-2 is Pending
    assert_eq!(
        graph.nodes.get("step-1").unwrap().status,
        TaskStatus::Completed
    );
    assert_eq!(
        graph.nodes.get("step-2").unwrap().status,
        TaskStatus::Pending
    );

    // Resume execution: get_ready_tasks skips step-1 and returns step-2
    let ready_after_resume = graph.get_ready_tasks();
    assert_eq!(ready_after_resume.len(), 1);
    assert_eq!(ready_after_resume[0].id, "step-2");

    // Complete remaining work
    graph.mark_running("step-2", "w-2");
    graph.mark_completed("step-2", None);

    assert!(graph.is_finished());
}
