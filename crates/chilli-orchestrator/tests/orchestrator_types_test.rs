use chilli_orchestrator::types::{AgentRole, PlannerTaskSpec, TaskNode, TaskPlan, TaskStatus};

#[test]
fn test_task_plan_deserialization() {
    let json_plan = r#"{
        "plan_id": "plan-01",
        "goal": "Refactor auth middleware",
        "tasks": [
            {
                "id": "task-1",
                "description": "Inspect JWT middleware",
                "role": "Planner",
                "depends_on": [],
                "expected_artifacts": ["auth_plan.json"]
            },
            {
                "id": "task-2",
                "description": "Update token verification",
                "role": "Worker",
                "depends_on": ["task-1"],
                "expected_artifacts": ["src/auth.rs"]
            }
        ]
    }"#;

    let plan: TaskPlan = serde_json::from_str(json_plan).expect("Parse TaskPlan");
    assert_eq!(plan.tasks.len(), 2);
    assert_eq!(plan.tasks[1].depends_on, vec!["task-1"]);
}

#[test]
fn test_task_node_creation() {
    let spec = PlannerTaskSpec {
        id: "t-1".to_string(),
        description: "Task 1".to_string(),
        role: AgentRole::Worker,
        depends_on: vec![],
        expected_artifacts: vec![],
    };

    let node = TaskNode {
        id: spec.id.clone(),
        spec,
        status: TaskStatus::Pending,
        output: None,
    };

    assert_eq!(node.status, TaskStatus::Pending);
}
