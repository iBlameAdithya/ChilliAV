use chilli_core::engine::AgentEngine;

#[tokio::test]
async fn test_agent_engine_duplicate_loop_termination() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut engine = AgentEngine::new(temp_dir.path()).unwrap();

    let tool_name = "read_file";
    let args = serde_json::json!({"path": "nonexistent.rs"});

    // Record N=1, 2, 3 failures
    for _ in 0..3 {
        let res = engine.record_tool_failure_for_loop_detector(tool_name, &args, "file not found");
        assert!(!res.is_terminate());
    }

    assert!(engine.is_tool_call_locked(tool_name, &args));

    // N=4 returns termination signal
    let res = engine.record_tool_failure_for_loop_detector(tool_name, &args, "file not found");
    assert!(res.is_terminate());
}
