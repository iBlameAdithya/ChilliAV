use chilli_orchestrator::graph::TaskGraph;
use chilli_orchestrator::types::{AgentRole, PlannerTaskSpec, TaskPlan};

#[test]
fn test_task_graph_dependency_resolution() {
    let plan = TaskPlan {
        plan_id: "p1".to_string(),
        goal: "Build feature".to_string(),
        tasks: vec![
            PlannerTaskSpec {
                id: "t1".to_string(),
                description: "Step 1".to_string(),
                role: AgentRole::Planner,
                depends_on: vec![],
                expected_artifacts: vec![],
            },
            PlannerTaskSpec {
                id: "t2".to_string(),
                description: "Step 2".to_string(),
                role: AgentRole::Worker,
                depends_on: vec!["t1".to_string()],
                expected_artifacts: vec![],
            },
        ],
    };

    let mut graph = TaskGraph::from_plan(plan).expect("Valid DAG");

    // Initially t1 should be ready, t2 pending
    let ready = graph.get_ready_tasks();
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].id, "t1");

    // Mark t1 completed
    graph.mark_completed("t1", None);

    // Now t2 should be ready
    let ready_next = graph.get_ready_tasks();
    assert_eq!(ready_next.len(), 1);
    assert_eq!(ready_next[0].id, "t2");
}

#[test]
fn test_task_graph_detects_cycle() {
    let plan = TaskPlan {
        plan_id: "cycle_plan".to_string(),
        goal: "Invalid cycle".to_string(),
        tasks: vec![
            PlannerTaskSpec {
                id: "t1".to_string(),
                description: "Task 1".to_string(),
                role: AgentRole::Worker,
                depends_on: vec!["t2".to_string()],
                expected_artifacts: vec![],
            },
            PlannerTaskSpec {
                id: "t2".to_string(),
                description: "Task 2".to_string(),
                role: AgentRole::Worker,
                depends_on: vec!["t1".to_string()],
                expected_artifacts: vec![],
            },
        ],
    };

    let result = TaskGraph::from_plan(plan);
    assert!(result.is_err());
}

#[test]
fn test_task_graph_bounded_concurrency() {
    let plan = TaskPlan {
        plan_id: "p_bounded".to_string(),
        goal: "Parallel execution".to_string(),
        tasks: vec![
            PlannerTaskSpec {
                id: "t1".to_string(),
                description: "Task 1".to_string(),
                role: AgentRole::Worker,
                depends_on: vec![],
                expected_artifacts: vec![],
            },
            PlannerTaskSpec {
                id: "t2".to_string(),
                description: "Task 2".to_string(),
                role: AgentRole::Worker,
                depends_on: vec![],
                expected_artifacts: vec![],
            },
            PlannerTaskSpec {
                id: "t3".to_string(),
                description: "Task 3".to_string(),
                role: AgentRole::Worker,
                depends_on: vec![],
                expected_artifacts: vec![],
            },
        ],
    };

    let graph = TaskGraph::from_plan(plan).expect("Valid DAG");
    let ready_all = graph.get_ready_tasks();
    assert_eq!(ready_all.len(), 3);

    let ready_bounded = graph.get_ready_tasks_bounded(2);
    assert_eq!(ready_bounded.len(), 2);
}

#[test]
fn test_task_graph_state_transitions() {
    let plan = TaskPlan {
        plan_id: "p_transitions".to_string(),
        goal: "State transitions".to_string(),
        tasks: vec![PlannerTaskSpec {
            id: "t1".to_string(),
            description: "Step 1".to_string(),
            role: AgentRole::Worker,
            depends_on: vec![],
            expected_artifacts: vec![],
        }],
    };

    let mut graph = TaskGraph::from_plan(plan).expect("Valid DAG");

    graph.mark_running("t1", "agent_1");
    assert!(matches!(
        graph.nodes.get("t1").unwrap().status,
        chilli_orchestrator::types::TaskStatus::Running { .. }
    ));

    graph.mark_verifying("t1", "verifier_1");
    assert!(matches!(
        graph.nodes.get("t1").unwrap().status,
        chilli_orchestrator::types::TaskStatus::Verifying { .. }
    ));

    graph.mark_verified("t1");
    assert_eq!(
        graph.nodes.get("t1").unwrap().status,
        chilli_orchestrator::types::TaskStatus::Verified
    );

    assert!(graph.is_finished());
}
