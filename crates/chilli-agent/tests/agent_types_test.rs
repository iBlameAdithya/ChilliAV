use chilli_agent::types::{AgentConfig, AgentEvent, AgentRole};
use chilli_model::router::Requirement;

#[test]
fn test_agent_config_serialization() {
    let config = AgentConfig {
        agent_id: "planner-01".to_string(),
        role: AgentRole::Planner,
        system_instruction_override: Some("Plan task".to_string()),
        allowed_tools: vec!["read_file".to_string()],
        max_turns: 5,
        model_requirements: vec![Requirement::MinContext(128_000)],
    };

    let json = serde_json::to_string(&config).expect("Serialize AgentConfig");
    let deserialized: AgentConfig = serde_json::from_str(&json).expect("Deserialize AgentConfig");
    assert_eq!(deserialized.agent_id, "planner-01");
    assert_eq!(deserialized.role, AgentRole::Planner);
}

#[test]
fn test_agent_event_lightweight() {
    let event = AgentEvent::Completed {
        task_id: "task-1".to_string(),
        agent_id: "worker-1".to_string(),
        success: true,
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("task-1"));
    assert!(json.contains("worker-1"));
}
