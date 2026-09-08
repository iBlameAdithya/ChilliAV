use chilli_context::builder::{format_checkpoint_resume_prompt, ContextBuilder};
use chilli_core::engine::{AgentEngine, MockResponse};
use chilli_core::state::TaskPhase;
use chilli_memory::task_store::{TaskCheckpointStore, CURRENT_SCHEMA_VERSION};
use chilli_memory::{CurrentWorkspaceState, TaskCheckpoint, TaskStatus};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

fn create_valid_checkpoint(
    task_id: &str,
    phase: &str,
    workspace_root: &std::path::Path,
) -> TaskCheckpoint {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    TaskCheckpoint {
        task_id: task_id.to_string(),
        goal: "Perform complex multi-phase task".to_string(),
        phase: phase.to_string(),
        status: TaskStatus::Active,
        workspace_root: workspace_root.to_string_lossy().to_string(),
        workspace_hash: CurrentWorkspaceState::compute_workspace_hash(workspace_root),
        created_at: now,
        updated_at: now,
        ..TaskCheckpoint::default()
    }
}

#[tokio::test]
async fn test_interruption_and_resume_across_all_phases() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_a = temp_dir.path().join("lib.rs");
    fs::write(&file_a, "pub fn add(a: i32, b: i32) -> i32 { a + b }").unwrap();

    let phases = vec!["understand", "inspect", "plan", "act", "verify", "recover"];

    for phase_str in phases {
        let mut engine = AgentEngine::new(temp_dir.path()).unwrap();
        engine.set_mock_responses(vec![
            MockResponse::ToolCall {
                name: "read_file".to_string(),
                arguments: serde_json::json!({ "path": "lib.rs" }),
            },
            MockResponse::Text(format!("Phase {} finished", phase_str)),
        ]);

        let cp = create_valid_checkpoint(
            &format!("task_phase_{}", phase_str),
            phase_str,
            temp_dir.path(),
        );

        let res = engine.resume_task_from_checkpoint(&cp).await;
        assert!(
            res.is_ok(),
            "Failed resuming at phase {}: {:?}",
            phase_str,
            res
        );

        let active_task = engine.active_task.as_ref().unwrap();
        assert_eq!(
            active_task.phase,
            TaskPhase::Finished,
            "Phase completion mismatch for {}",
            phase_str
        );
    }
}

#[tokio::test]
async fn test_repeated_interruption_loops() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_a = temp_dir.path().join("main.rs");
    fs::write(&file_a, "fn main() {}").unwrap();

    let store = TaskCheckpointStore::new(temp_dir.path()).unwrap();

    let mut cp = create_valid_checkpoint("task_loop_1", "understand", temp_dir.path());
    cp.visited_files.push("main.rs".to_string());
    store.save_checkpoint(&cp).unwrap();

    let stages = vec![
        ("inspect", vec!["edit_file"]),
        ("plan", vec!["read_file"]),
        ("act", vec!["edit_file"]),
        ("verify", vec!["read_file"]),
    ];

    for (next_phase, tool_calls) in stages {
        let loaded_cp = store.load_checkpoint("task_loop_1").unwrap();
        let mut engine = AgentEngine::new(temp_dir.path()).unwrap();

        let mut mocks = Vec::new();
        for tool in &tool_calls {
            mocks.push(MockResponse::ToolCall {
                name: tool.to_string(),
                arguments: serde_json::json!({ "path": "main.rs", "content": "// updated" }),
            });
        }
        mocks.push(MockResponse::Text(format!("Completed {}", next_phase)));
        engine.set_mock_responses(mocks);

        let res = engine.resume_task_from_checkpoint(&loaded_cp).await;
        assert!(res.is_ok(), "Failed at step {}", next_phase);

        let mut updated_cp = loaded_cp;
        updated_cp.phase = next_phase.to_string();
        updated_cp.visited_files.push("main.rs".to_string());
        updated_cp.updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        store.save_checkpoint(&updated_cp).unwrap();
    }

    let final_cp = store.load_checkpoint("task_loop_1").unwrap();
    assert_eq!(final_cp.phase, "verify");
}

#[tokio::test]
async fn test_safety_budget_non_reset_across_resumptions() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut engine = AgentEngine::new(temp_dir.path()).unwrap();

    engine.set_mock_responses(vec![MockResponse::Text(
        "Resumed turn complete".to_string(),
    )]);

    let mut cp = create_valid_checkpoint("task_budget_test", "act", temp_dir.path());
    cp.verification_attempts = 4;
    cp.verification_failures = vec!["Failure 1".to_string(), "Failure 2".to_string()];
    cp.executed_tools_summary = vec!["edit_file".to_string(), "read_file".to_string()];

    let res = engine.resume_task_from_checkpoint(&cp).await;
    assert!(res.is_ok());

    let active_task = engine.active_task.as_ref().unwrap();
    assert_eq!(active_task.verification_attempts, 4);
    assert_eq!(
        active_task.verification_failures,
        vec!["Failure 1".to_string(), "Failure 2".to_string()]
    );
    assert_eq!(active_task.consecutive_failures, 0);
}

#[tokio::test]
async fn test_non_replay_of_completed_destructive_tools() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_a = temp_dir.path().join("target.txt");
    fs::write(&file_a, "initial state").unwrap();

    let mut cp = create_valid_checkpoint("task_no_replay", "act", temp_dir.path());
    cp.executed_tools_summary = vec!["edit_file: target.txt".to_string()];
    cp.visited_files = vec!["target.txt".to_string()];

    let mut engine = AgentEngine::new(temp_dir.path()).unwrap();
    engine.set_mock_responses(vec![MockResponse::Text("Skipped tool call".to_string())]);

    let res = engine.resume_task_from_checkpoint(&cp).await;
    assert!(res.is_ok());

    let content = fs::read_to_string(&file_a).unwrap();
    assert_eq!(content, "initial state");
}

#[tokio::test]
async fn test_process_restart_persistence_via_sqlite() {
    let temp_dir = tempfile::tempdir().unwrap();

    let mut original_cp = create_valid_checkpoint("persisted_task_123", "inspect", temp_dir.path());
    original_cp.goal = "Refactor database module".to_string();
    original_cp.visited_files = vec!["src/db.rs".to_string()];
    original_cp.modified_files = vec!["src/lib.rs".to_string()];
    original_cp.verification_attempts = 3;
    original_cp.verification_failures = vec!["type error on line 42".to_string()];

    {
        let store = TaskCheckpointStore::new(temp_dir.path()).unwrap();
        store.save_checkpoint(&original_cp).unwrap();
    }

    // Simulate process restart by creating a new store instance
    let new_store = TaskCheckpointStore::new(temp_dir.path()).unwrap();
    let loaded = new_store
        .load_checkpoint("persisted_task_123")
        .expect("Checkpoint should exist after restart");

    assert_eq!(loaded.task_id, "persisted_task_123");
    assert_eq!(loaded.goal, "Refactor database module");
    assert_eq!(loaded.phase, "inspect");
    assert_eq!(loaded.visited_files, vec!["src/db.rs".to_string()]);
    assert_eq!(loaded.modified_files, vec!["src/lib.rs".to_string()]);
    assert_eq!(loaded.verification_attempts, 3);
    assert_eq!(
        loaded.verification_failures,
        vec!["type error on line 42".to_string()]
    );
}

#[test]
fn test_context_reconstruction_token_limit_constraint() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut builder = ContextBuilder::new(20000);

    let mut cp = create_valid_checkpoint("task_ctx_limit", "inspect", temp_dir.path());
    cp.visited_files = (0..100).map(|i| format!("file_{}.rs", i)).collect();
    cp.executed_tools_summary = (0..100)
        .map(|i| format!("executed_tool_{}_with_very_long_summary_payload", i))
        .collect();

    let notices = vec![
        "External change in file_a.rs".to_string(),
        "Branch divergence detected".to_string(),
    ];

    let _telemetry = builder.reconstruct_from_checkpoint(&cp, &notices, None, None);
    let resume_prompt = format_checkpoint_resume_prompt(&cp, &notices);

    assert!(!resume_prompt.is_empty());
    let approx_tokens = resume_prompt.len() / 4;
    assert!(
        approx_tokens <= 1500,
        "Tier-3 reconstructed context exceeds 1500 tokens budget limit: approx {} tokens",
        approx_tokens
    );
}

#[tokio::test]
async fn test_corrupt_and_stale_checkpoint_rejection() {
    let temp_dir = tempfile::tempdir().unwrap();
    let engine = AgentEngine::new(temp_dir.path()).unwrap();

    // 1. Empty task_id (Corrupt)
    let corrupt_cp = create_valid_checkpoint("", "act", temp_dir.path());
    let res_corrupt = engine.evaluate_checkpoint_recovery_gate(&corrupt_cp);
    assert!(res_corrupt.is_err());
    assert!(res_corrupt.unwrap_err().contains("corrupt checkpoint data"));

    // 2. Unsupported schema version
    let mut bad_schema_cp = create_valid_checkpoint("bad_schema", "act", temp_dir.path());
    bad_schema_cp.schema_version = CURRENT_SCHEMA_VERSION + 99;
    let res_schema = engine.evaluate_checkpoint_recovery_gate(&bad_schema_cp);
    assert!(res_schema.is_err());
    assert!(res_schema.unwrap_err().contains("unsupported workspace"));

    // 3. Stale checkpoint (> 7 days)
    let mut stale_cp = create_valid_checkpoint("stale_task", "act", temp_dir.path());
    let ten_days_secs = 10 * 24 * 3600;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    stale_cp.updated_at = now.saturating_sub(ten_days_secs);
    let res_stale = engine.evaluate_checkpoint_recovery_gate(&stale_cp);
    assert!(res_stale.is_err());
    assert!(res_stale.unwrap_err().contains("checkpoint stale"));
}

#[tokio::test]
async fn test_external_workspace_conflict_detection() {
    let temp_dir = tempfile::tempdir().unwrap();

    // Initialize git repo for porcelain check
    let _ = std::process::Command::new("git")
        .args(["init"])
        .current_dir(temp_dir.path())
        .output();

    // Create file visited by checkpoint
    let visited_file = temp_dir.path().join("visited.rs");
    fs::write(&visited_file, "fn original() {}").unwrap();

    let mut cp = create_valid_checkpoint("task_conflict", "act", temp_dir.path());
    cp.visited_files = vec!["visited.rs".to_string()];

    // External modification to visited file
    fs::write(&visited_file, "fn modified_by_external_user() {}").unwrap();

    let engine = AgentEngine::new(temp_dir.path()).unwrap();
    let res = engine.evaluate_checkpoint_recovery_gate(&cp);

    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(
        err.contains("external modifications detected"),
        "Unexpected error: {}",
        err
    );
}
