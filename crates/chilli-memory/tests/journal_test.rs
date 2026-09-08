use chilli_memory::journal::{EventJournal, EventRecord};
use chilli_memory::storage::StoragePaths;
use tempfile::tempdir;

#[test]
fn test_event_journal_append_and_recovery() {
    let temp_workspace = tempdir().unwrap();
    let paths = StoragePaths::resolve(temp_workspace.path()).unwrap();

    let journal = EventJournal::open(&paths.journal_db_path).unwrap();

    let rec1 = EventRecord {
        seq: 0, // Auto-assigned by journal
        session_id: "sess_101".to_string(),
        event_type: "session.started".to_string(),
        payload_json: r#"{"user":"dev"}"#.to_string(),
        timestamp_ms: 1700000000000,
    };

    let rec2 = EventRecord {
        seq: 0,
        session_id: "sess_101".to_string(),
        event_type: "tool.started".to_string(),
        payload_json: r#"{"tool":"read_file"}"#.to_string(),
        timestamp_ms: 1700000001000,
    };

    let seq1 = journal.append(&rec1).unwrap();
    let seq2 = journal.append(&rec2).unwrap();

    assert_eq!(seq1, 1);
    assert_eq!(seq2, 2);

    drop(journal);

    // Re-open journal and verify recovery
    let recovered_journal = EventJournal::open(&paths.journal_db_path).unwrap();
    let events = recovered_journal.events_after("sess_101", 0).unwrap();

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].seq, 1);
    assert_eq!(events[0].event_type, "session.started");
    assert_eq!(events[1].seq, 2);
    assert_eq!(events[1].event_type, "tool.started");

    let events_since_1 = recovered_journal.events_after("sess_101", 1).unwrap();
    assert_eq!(events_since_1.len(), 1);
    assert_eq!(events_since_1[0].seq, 2);
}
