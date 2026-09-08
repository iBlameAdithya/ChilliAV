use chilli_core::engine::{AgentEngine, MockResponse};
use chilli_memory::{CurrentWorkspaceState, TaskCheckpoint, TaskStatus};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::test]
async fn test_resume_task_from_checkpoint_phase_reentry() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_a = temp_dir.path().join("main.rs");
    fs::write(&file_a, "fn main() {}").unwrap();

    let mut engine = AgentEngine::new(temp_dir.path()).unwrap();
    engine.set_mock_responses(vec![
        MockResponse::ToolCall {
            name: "edit_file".to_string(),
            arguments: serde_json::json!({
                "path": "main.rs",
                "content": "fn main() { println!(\"hello\"); }"
            }),
        },
        MockResponse::Text("Successfully edited main.rs".to_string()),
    ]);

    let cp = TaskCheckpoint {
        task_id: "task_reentry_1".to_string(),
        goal: "Update main.rs".to_string(),
        phase: "inspect".to_string(),
        status: TaskStatus::Active,
        visited_files: vec!["main.rs".to_string()],
        workspace_root: temp_dir.path().to_string_lossy().to_string(),
        workspace_hash: CurrentWorkspaceState::compute_workspace_hash(temp_dir.path()),
        updated_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        ..TaskCheckpoint::default()
    };

    let result = engine.resume_task_from_checkpoint(&cp).await;
    assert!(result.is_ok(), "Expected Ok, got {:?}", result);
    let res = result.unwrap();
    assert!(res.completed);
    assert_eq!(res.final_output, "Successfully edited main.rs");

    // Verify task state phase progressed
    let active_task = engine.active_task.as_ref().unwrap();
    assert_eq!(active_task.phase, chilli_core::state::TaskPhase::Finished);
    assert!(active_task.modified_files.contains(&"main.rs".to_string()));
}

#[tokio::test]
async fn test_resume_task_from_checkpoint_preserves_budgets() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut engine = AgentEngine::new(temp_dir.path()).unwrap();

    engine.set_mock_responses(vec![MockResponse::Text("Task complete".to_string())]);

    let cp = TaskCheckpoint {
        task_id: "task_budget_reentry".to_string(),
        goal: "Resume task".to_string(),
        phase: "recover".to_string(),
        status: TaskStatus::Active,
        verification_attempts: 2,
        verification_failures: vec!["previous failure".to_string()],
        workspace_root: temp_dir.path().to_string_lossy().to_string(),
        workspace_hash: CurrentWorkspaceState::compute_workspace_hash(temp_dir.path()),
        updated_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        ..TaskCheckpoint::default()
    };

    let result = engine.resume_task_from_checkpoint(&cp).await;
    assert!(result.is_ok());

    let active_task = engine.active_task.as_ref().unwrap();
    assert_eq!(active_task.verification_attempts, 2);
    assert_eq!(
        active_task.verification_failures,
        vec!["previous failure".to_string()]
    );
}

#[tokio::test]
async fn test_resume_task_from_checkpoint_rejected_if_completed() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut engine = AgentEngine::new(temp_dir.path()).unwrap();

    let cp = TaskCheckpoint {
        task_id: "task_completed".to_string(),
        goal: "Done task".to_string(),
        phase: "finished".to_string(),
        status: TaskStatus::Completed,
        workspace_root: temp_dir.path().to_string_lossy().to_string(),
        updated_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        ..TaskCheckpoint::default()
    };

    let result = engine.resume_task_from_checkpoint(&cp).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Cannot resume completed task"));
}
