use chilli_memory::journal::{EventJournal, EventRecord};
use chilli_memory::storage::StoragePaths;

#[tokio::test]
async fn test_resilience_tool_failure_and_resume_recovery() {
    let ws = std::env::temp_dir().join("chilli_recovery_ws");
    let _ = std::fs::remove_dir_all(&ws);
    std::fs::create_dir_all(&ws).unwrap();

    let storage = StoragePaths::resolve(&ws).unwrap();
    std::fs::write(ws.join("config.json"), "{\"status\": \"pending\"}").unwrap();

    // 1. Simulate crash after CheckpointSaved event
    {
        let journal = EventJournal::open(&storage.journal_db_path).unwrap();
        journal
            .append(&EventRecord {
                seq: 1,
                session_id: "default_session".to_string(),
                event_type: "TurnStarted".to_string(),
                payload_json: "{\"turn\":1}".to_string(),
                timestamp_ms: 1000,
            })
            .unwrap();
        journal
            .append(&EventRecord {
                seq: 2,
                session_id: "default_session".to_string(),
                event_type: "ToolExecuted".to_string(),
                payload_json: "{\"tool\":\"write_file\",\"path\":\"config.json\"}".to_string(),
                timestamp_ms: 1001,
            })
            .unwrap();
        journal
            .append(&EventRecord {
                seq: 3,
                session_id: "default_session".to_string(),
                event_type: "CheckpointSaved".to_string(),
                payload_json: "{\"checkpoint_id\":\"cp_crash\"}".to_string(),
                timestamp_ms: 1002,
            })
            .unwrap();
        journal
            .append(&EventRecord {
                seq: 4,
                session_id: "default_session".to_string(),
                event_type: "TurnStarted".to_string(),
                payload_json: "{\"turn\":2}".to_string(),
                timestamp_ms: 1003,
            })
            .unwrap();
        // Process killed here before completion event
    }

    // 2. Resume execution using AgentEngine
    let mut engine = chilli_core::engine::AgentEngine::new(&ws).unwrap();
    let res = engine.resume_task().await.unwrap();

    assert!(res.recovered);
    assert_eq!(res.last_checkpoint_id, "cp_crash");
}
