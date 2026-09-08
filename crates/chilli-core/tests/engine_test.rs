#[tokio::test]
async fn test_agent_engine_state_machine_mock_loop() {
    let temp_dir = std::env::temp_dir().join("chilli_core_engine_ws");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let mut engine = chilli_core::engine::AgentEngine::new(&temp_dir).unwrap();

    // Configure mock model response returning write_file tool call then finish text
    engine.set_mock_responses(vec![
        chilli_core::engine::MockResponse::ToolCall {
            name: "write_file".to_string(),
            arguments: serde_json::json!({"path": "result.txt", "content": "core engine output"}),
        },
        chilli_core::engine::MockResponse::Text("Task completed successfully.".to_string()),
    ]);

    let result = engine
        .run_task("Create result.txt with core engine output")
        .await
        .unwrap();

    assert!(result.completed);
    assert_eq!(result.final_output, "Task completed successfully.");
    assert!(temp_dir.join("result.txt").exists());
    assert_eq!(
        std::fs::read_to_string(temp_dir.join("result.txt")).unwrap(),
        "core engine output"
    );

    // Verify journal events persisted
    let journal_events = engine.journal().events_after("default_session", 0).unwrap();
    assert!(journal_events
        .iter()
        .any(|e| e.event_type == "ToolExecuted"));
}

#[tokio::test]
async fn test_agent_engine_uses_cache_aligned_context() {
    let temp_dir = std::env::temp_dir().join("test_agent_engine_cache_ctx");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let mut engine = chilli_core::engine::AgentEngine::new(&temp_dir).unwrap();
    engine.set_mock_responses(vec![chilli_core::engine::MockResponse::Text(
        "Done".to_string(),
    )]);

    let res = engine
        .run_task("Explain cache aligned context")
        .await
        .unwrap();
    assert!(res.completed);
    assert_eq!(res.final_output, "Done");
}

#[tokio::test]
async fn test_agent_engine_zero_tool_guard_retry() {
    let temp_dir = std::env::temp_dir().join("test_agent_engine_zero_tool");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let mut engine = chilli_core::engine::AgentEngine::new(&temp_dir).unwrap();
    // First turn returns text with 0 tool calls for a workspace task ("Create output.txt")
    // Second turn uses write_file tool call
    // Third turn finishes task
    engine.set_mock_responses(vec![
        chilli_core::engine::MockResponse::Text("I will try to explain instead of tool call".to_string()),
        chilli_core::engine::MockResponse::ToolCall {
            name: "write_file".to_string(),
            arguments: serde_json::json!({"path": "output.txt", "content": "zero tool recovery success"}),
        },
        chilli_core::engine::MockResponse::Text("Created output.txt successfully.".to_string()),
    ]);

    let res = engine
        .run_task("Create output.txt with contents zero tool recovery success")
        .await
        .unwrap();

    assert!(res.completed);
    assert_eq!(res.final_output, "Created output.txt successfully.");
    assert!(temp_dir.join("output.txt").exists());
}

#[tokio::test]
async fn test_agent_engine_verification_recovery_loop() {
    let temp_dir = std::env::temp_dir().join("test_agent_engine_v_recovery");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let mut engine = chilli_core::engine::AgentEngine::new(&temp_dir).unwrap();

    // Set verification spec requiring output.txt to contain "pass"
    engine.set_verification_spec(chilli_core::verifier::VerificationSpec {
        checks: vec![chilli_core::verifier::VerificationType::FileContains {
            path: "output.txt".to_string(),
            expected: "pass".to_string(),
        }],
    });

    // Turn 1: tool call write_file output.txt with "fail" -> verifier fails -> recovery attempt 1 feedback injected
    // Turn 2: tool call write_file output.txt with "pass" -> verifier succeeds -> finish
    engine.set_mock_responses(vec![
        chilli_core::engine::MockResponse::ToolCall {
            name: "write_file".to_string(),
            arguments: serde_json::json!({"path": "output.txt", "content": "fail"}),
        },
        chilli_core::engine::MockResponse::Text("Done first attempt".to_string()),
        chilli_core::engine::MockResponse::ToolCall {
            name: "write_file".to_string(),
            arguments: serde_json::json!({"path": "output.txt", "content": "pass"}),
        },
        chilli_core::engine::MockResponse::Text("Task fixed and completed.".to_string()),
    ]);

    let res = engine
        .run_task("Fix issue in output.txt so verification passes")
        .await
        .unwrap();

    assert!(res.completed);
    assert_eq!(res.final_output, "Task fixed and completed.");
    assert_eq!(
        std::fs::read_to_string(temp_dir.join("output.txt")).unwrap(),
        "pass"
    );
}

#[test]
fn test_tools_declaration_for_phase() {
    use chilli_core::engine::AgentEngine;
    use chilli_core::state::TaskPhase;

    let expected_tools = vec![
        "read_file",
        "write_file",
        "edit_file",
        "grep",
        "list_directory",
        "exec",
        "move_file",
        "copy_file",
        "create_directory",
        "remove_file",
        "find_symbol",
        "find_references",
        "find_test_for_failure",
    ];

    let understand_tools = AgentEngine::tools_declaration_for_phase(TaskPhase::Understand);
    let understand_names: Vec<&str> = understand_tools
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.get("name").unwrap().as_str().unwrap())
        .collect();
    assert_eq!(understand_names, expected_tools);

    let verify_tools = AgentEngine::tools_declaration_for_phase(TaskPhase::Verify);
    let verify_names: Vec<&str> = verify_tools
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.get("name").unwrap().as_str().unwrap())
        .collect();
    assert_eq!(verify_names, expected_tools);

    let execute_tools = AgentEngine::tools_declaration_for_phase(TaskPhase::Execute);
    let execute_names: Vec<&str> = execute_tools
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.get("name").unwrap().as_str().unwrap())
        .collect();
    assert_eq!(execute_names, expected_tools);
}

#[tokio::test]
async fn test_pipeline_failure_counter_sync() {
    let temp_dir = std::env::temp_dir().join("test_pipeline_failure_sync");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let mut engine = chilli_core::engine::AgentEngine::new(&temp_dir).unwrap();

    // Turn 1: tool call read_file non-existent file -> fails -> consecutive_failures = 1
    // Turn 2: tool call write_file output.txt -> succeeds -> consecutive_failures = 0
    // Turn 3: finish text
    engine.set_mock_responses(vec![
        chilli_core::engine::MockResponse::ToolCall {
            name: "read_file".to_string(),
            arguments: serde_json::json!({"path": "non_existent.txt"}),
        },
        chilli_core::engine::MockResponse::ToolCall {
            name: "write_file".to_string(),
            arguments: serde_json::json!({"path": "output.txt", "content": "sync test"}),
        },
        chilli_core::engine::MockResponse::Text("Completed failure counter sync test.".to_string()),
    ]);

    let res = engine
        .run_task("Test pipeline failure counter reset on success")
        .await
        .unwrap();

    assert!(res.completed);
    assert_eq!(engine.active_task.as_ref().unwrap().consecutive_failures, 0);
}

#[test]
fn test_small_model_prompt_contains_tool_creation_rules() {
    let temp = std::env::temp_dir();
    let mut engine = chilli_core::engine::AgentEngine::new(&temp).unwrap();
    engine.prompt_profile = chilli_core::engine::PromptProfile::SmallModel;
    let prompt = engine.build_system_prompt();
    assert!(prompt.contains("use write_file directly"));
}
