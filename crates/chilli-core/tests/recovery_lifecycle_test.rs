use chilli_core::engine::AgentEngine;
use chilli_memory::journal::{EventJournal, EventRecord};
use chilli_memory::storage::StoragePaths;

#[tokio::test]
async fn test_recovery_operation_correlation_lifecycle() {
    let ws = std::env::temp_dir().join("chilli_recovery_lifecycle_test");
    let _ = std::fs::remove_dir_all(&ws);
    std::fs::create_dir_all(&ws).unwrap();

    let storage = StoragePaths::resolve(&ws).unwrap();
    let journal = EventJournal::open(&storage.journal_db_path).unwrap();

    let op_id = "op_12345";

    // Simulate checkpoint event first
    journal
        .append(&EventRecord {
            seq: 1,
            session_id: "default_session".to_string(),
            event_type: "CheckpointSaved".to_string(),
            payload_json: serde_json::json!({ "checkpoint_id": "cp_001" }).to_string(),
            timestamp_ms: 100,
        })
        .unwrap();

    // Event 2: ToolCallStarted
    journal
        .append(&EventRecord {
            seq: 2,
            session_id: "default_session".to_string(),
            event_type: "ToolCallStarted".to_string(),
            payload_json: serde_json::json!({
                "op_id": op_id,
                "tool": "write_file",
                "arguments": { "path": "test.txt", "content": "data" }
            })
            .to_string(),
            timestamp_ms: 101,
        })
        .unwrap();

    // Without ToolCallCommitted or ToolResultRecorded, simulate crash recovery
    let mut engine = AgentEngine::new(&ws).unwrap();
    let res = engine.resume_task().await.unwrap();

    assert!(res.recovered);
    assert_eq!(res.last_checkpoint_id, "cp_001");
    assert_eq!(res.uncommitted_operations.len(), 1);
    assert_eq!(res.uncommitted_operations[0], op_id);
}
