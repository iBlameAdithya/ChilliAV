use chilli_memory::{
    CompactTaskTelemetry, TaskCheckpoint, TaskCheckpointStore, TaskStatus, TaskStoreError,
    CURRENT_SCHEMA_VERSION,
};
use tempfile::tempdir;

fn sample_checkpoint(id: &str, status: TaskStatus) -> TaskCheckpoint {
    TaskCheckpoint {
        task_id: id.to_string(),
        workspace_root: "/test/workspace".to_string(),
        workspace_hash: "hash123".to_string(),
        git_commit: Some("abc123def".to_string()),
        goal: "Implement task persistence".to_string(),
        phase: "implementation".to_string(),
        status,
        execution_strategy: Some("direct".to_string()),
        completed_subtasks: vec!["setup storage".to_string()],
        pending_subtasks: vec!["write tests".to_string()],
        modified_files: vec!["src/task_store.rs".to_string()],
        visited_files: vec!["src/storage.rs".to_string()],
        verification_attempts: 1,
        verification_failures: vec![],
        executed_tools_summary: vec!["write".to_string()],
        telemetry: CompactTaskTelemetry {
            input_tokens: 500,
            output_tokens: 100,
            cached_tokens: 50,
            total_tokens: 650,
            total_turns: 2,
            tool_calls_count: 3,
        },
        schema_version: CURRENT_SCHEMA_VERSION,
        created_at: 1000,
        updated_at: 1000,
    }
}

#[test]
fn test_task_checkpoint_crud_operations() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("tasks.db");
    let store = TaskCheckpointStore::with_custom_path(&db_path).unwrap();

    let cp1 = sample_checkpoint("task-1", TaskStatus::Active);
    store.save_checkpoint(&cp1).unwrap();

    let loaded = store.load_checkpoint("task-1").unwrap();
    assert_eq!(loaded.task_id, "task-1");
    assert_eq!(loaded.status, TaskStatus::Active);
    assert_eq!(loaded.goal, "Implement task persistence");
    assert_eq!(loaded.telemetry.total_tokens, 650);

    // Delete
    let deleted = store.delete_checkpoint("task-1").unwrap();
    assert!(deleted);

    let err = store.load_checkpoint("task-1").unwrap_err();
    assert!(matches!(err, TaskStoreError::NotFound(_)));
}

#[test]
fn test_atomic_checkpoint_upsert() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("tasks.db");
    let store = TaskCheckpointStore::with_custom_path(&db_path).unwrap();

    let mut cp = sample_checkpoint("task-1", TaskStatus::Active);
    store.save_checkpoint(&cp).unwrap();

    // Update fields
    cp.status = TaskStatus::Completed;
    cp.completed_subtasks.push("write tests".to_string());
    cp.pending_subtasks.clear();

    store.save_checkpoint(&cp).unwrap();

    let loaded = store.load_checkpoint("task-1").unwrap();
    assert_eq!(loaded.status, TaskStatus::Completed);
    assert_eq!(loaded.completed_subtasks.len(), 2);
    assert!(loaded.pending_subtasks.is_empty());
}

#[test]
fn test_task_status_parsing_and_fail_closed() {
    assert_eq!(TaskStatus::parse("active").unwrap(), TaskStatus::Active);
    assert_eq!(TaskStatus::parse("PAUSED").unwrap(), TaskStatus::Paused);
    assert_eq!(
        TaskStatus::parse("interrupted").unwrap(),
        TaskStatus::Interrupted
    );
    assert_eq!(
        TaskStatus::parse("recoverable").unwrap(),
        TaskStatus::Recoverable
    );
    assert_eq!(
        TaskStatus::parse("completed").unwrap(),
        TaskStatus::Completed
    );
    assert_eq!(TaskStatus::parse("failed").unwrap(), TaskStatus::Failed);

    let active_cp = sample_checkpoint("cp-active", TaskStatus::Active);
    assert!(!active_cp.is_terminal());
    assert_eq!(active_cp.subtask_completion_rate(), 0.5);

    let completed_cp = sample_checkpoint("cp-completed", TaskStatus::Completed);
    assert!(completed_cp.is_terminal());
    assert_eq!(
        TaskStatus::parse("abandoned").unwrap(),
        TaskStatus::Abandoned
    );

    let err = TaskStatus::parse("invalid_status_value").unwrap_err();
    assert!(matches!(err, TaskStoreError::UnknownStatus(_)));
}

#[test]
fn test_credential_and_sensitive_file_sanitization() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("tasks.db");
    let store = TaskCheckpointStore::with_custom_path(&db_path).unwrap();

    let mut cp = sample_checkpoint("task-secret", TaskStatus::Active);
    cp.goal = "Read secret API_KEY=secret_12345".to_string();
    cp.modified_files = vec![
        "src/main.rs".to_string(),
        ".env".to_string(),
        "config/.env.local".to_string(),
        "~/.ssh/id_rsa".to_string(),
    ];
    cp.visited_files = vec!["src/lib.rs".to_string(), "keys/id_ed25519".to_string()];
    cp.completed_subtasks = vec!["set AWS_SECRET_ACCESS_KEY=abcd".to_string()];

    store.save_checkpoint(&cp).unwrap();

    let loaded = store.load_checkpoint("task-secret").unwrap();

    // Sensitive files removed
    assert_eq!(loaded.modified_files, vec!["src/main.rs"]);
    assert_eq!(loaded.visited_files, vec!["src/lib.rs"]);

    // Secret text redacted
    assert!(loaded.goal.contains("[REDACTED_CREDENTIAL"));
    assert!(loaded.completed_subtasks[0].contains("[REDACTED_CREDENTIAL"));
}

#[test]
fn test_query_recoverable_tasks_and_status_filter() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("tasks.db");
    let store = TaskCheckpointStore::with_custom_path(&db_path).unwrap();

    store
        .save_checkpoint(&sample_checkpoint("task-1", TaskStatus::Active))
        .unwrap();
    store
        .save_checkpoint(&sample_checkpoint("task-2", TaskStatus::Interrupted))
        .unwrap();
    store
        .save_checkpoint(&sample_checkpoint("task-3", TaskStatus::Completed))
        .unwrap();
    store
        .save_checkpoint(&sample_checkpoint("task-4", TaskStatus::Failed))
        .unwrap();

    let recoverable = store.query_recoverable_tasks().unwrap();
    assert_eq!(recoverable.len(), 2);
    let ids: Vec<&str> = recoverable.iter().map(|c| c.task_id.as_str()).collect();
    assert!(ids.contains(&"task-1"));
    assert!(ids.contains(&"task-2"));

    let completed_only = store.list_checkpoints(Some(TaskStatus::Completed)).unwrap();
    assert_eq!(completed_only.len(), 1);
    assert_eq!(completed_only[0].task_id, "task-3");
}

#[test]
fn test_schema_version_compatibility_check() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("tasks.db");
    let store = TaskCheckpointStore::with_custom_path(&db_path).unwrap();

    let mut cp = sample_checkpoint("future-task", TaskStatus::Active);
    cp.schema_version = CURRENT_SCHEMA_VERSION + 10;

    let err = store.save_checkpoint(&cp).unwrap_err();
    assert!(matches!(
        err,
        TaskStoreError::UnsupportedSchemaVersion { .. }
    ));
}
