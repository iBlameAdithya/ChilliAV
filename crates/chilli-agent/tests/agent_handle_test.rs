use chilli_agent::handle::AgentHandle;
use chilli_agent::types::{AgentConfig, AgentEvent, AgentInput, AgentRole};
use tokio::sync::mpsc;

#[tokio::test]
async fn test_agent_handle_event_stream() {
    let (tx, mut rx) = mpsc::channel(16);
    let config = AgentConfig {
        agent_id: "test-worker".to_string(),
        role: AgentRole::Worker,
        system_instruction_override: None,
        allowed_tools: vec!["read_file".to_string()],
        max_turns: 3,
        model_requirements: vec![],
    };

    let handle = AgentHandle::new(config, tx);
    let input = AgentInput {
        task_id: "t-100".to_string(),
        prompt: "Fix bug".to_string(),
        context_files: vec![],
        structured_data: None,
    };

    // Execute mock run
    let output = handle.run_mock(input).await.unwrap();
    assert_eq!(output.task_id, "t-100");
    assert!(output.success);

    // Verify events received on channel
    let first_event = rx.recv().await.expect("Receive event");
    match first_event {
        AgentEvent::Started {
            task_id, agent_id, ..
        } => {
            assert_eq!(task_id, "t-100");
            assert_eq!(agent_id, "test-worker");
        }
        _ => panic!("Expected AgentEvent::Started"),
    }
}
