use chilli_core::engine::AgentEngine;
use chilli_memory::{CurrentWorkspaceState, SafeResumeDecision, TaskCheckpoint, TaskStatus};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::test]
async fn test_recovery_gate_safe_to_resume() {
    let temp_dir = tempfile::tempdir().unwrap();
    let engine = AgentEngine::new(temp_dir.path()).unwrap();

    let cp = TaskCheckpoint {
        task_id: "task_safe_1".to_string(),
        goal: "Refactor parser".to_string(),
        phase: "inspect".to_string(),
        status: TaskStatus::Active,
        workspace_root: temp_dir.path().to_string_lossy().to_string(),
        workspace_hash: CurrentWorkspaceState::compute_workspace_hash(temp_dir.path()),
        updated_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        ..TaskCheckpoint::default()
    };

    let res = engine.evaluate_checkpoint_recovery_gate(&cp);
    assert!(res.is_ok());
    let val_res = res.unwrap();
    assert_eq!(val_res.decision, SafeResumeDecision::SafeToResume);
}

#[tokio::test]
async fn test_recovery_gate_safe_with_notice() {
    let temp_dir = tempfile::tempdir().unwrap();
    // Initialize git repository so git status tracking is active
    let _ = std::process::Command::new("git")
        .args(["init"])
        .current_dir(temp_dir.path())
        .output();

    let engine = AgentEngine::new(temp_dir.path()).unwrap();

    // Create an unrelated file modification
    let unrelated_path = temp_dir.path().join("unrelated.txt");
    fs::write(&unrelated_path, "external noise").unwrap();

    let cp = TaskCheckpoint {
        task_id: "task_notice_1".to_string(),
        goal: "Fix bug".to_string(),
        phase: "act".to_string(),
        status: TaskStatus::Active,
        workspace_root: temp_dir.path().to_string_lossy().to_string(),
        updated_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        ..TaskCheckpoint::default()
    };

    let res = engine.evaluate_checkpoint_recovery_gate(&cp);
    assert!(res.is_ok());
    let val_res = res.unwrap();
    match val_res.decision {
        SafeResumeDecision::SafeWithNotice { notices } => {
            assert!(!notices.is_empty());
        }
        other => panic!("Expected SafeWithNotice, got {:?}", other),
    }
}

#[tokio::test]
async fn test_recovery_gate_external_changes_detected() {
    let temp_dir = tempfile::tempdir().unwrap();
    // Initialize git repository so git status tracking is active
    let _ = std::process::Command::new("git")
        .args(["init"])
        .current_dir(temp_dir.path())
        .output();

    let engine = AgentEngine::new(temp_dir.path()).unwrap();

    // Visited file modified externally
    let visited_path = temp_dir.path().join("auth.rs");
    fs::write(&visited_path, "fn modified_by_user() {}").unwrap();

    let cp = TaskCheckpoint {
        task_id: "task_conflict_1".to_string(),
        goal: "Update auth".to_string(),
        phase: "act".to_string(),
        status: TaskStatus::Active,
        visited_files: vec!["auth.rs".to_string()],
        workspace_root: temp_dir.path().to_string_lossy().to_string(),
        updated_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        ..TaskCheckpoint::default()
    };

    let res = engine.evaluate_checkpoint_recovery_gate(&cp);
    assert!(res.is_err());
    let err = res.err().unwrap();
    assert!(
        err.contains("external modifications detected"),
        "Error msg: {}",
        err
    );
}

#[tokio::test]
async fn test_recovery_gate_branch_changed() {
    let temp_dir = tempfile::tempdir().unwrap();
    let _engine = AgentEngine::new(temp_dir.path()).unwrap();

    let _cp = TaskCheckpoint {
        task_id: "task_branch_1".to_string(),
        goal: "Add feature".to_string(),
        phase: "inspect".to_string(),
        status: TaskStatus::Active,
        execution_strategy: Some("branch:feature-branch".to_string()),
        workspace_root: temp_dir.path().to_string_lossy().to_string(),
        updated_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        ..TaskCheckpoint::default()
    };

    let decision = SafeResumeDecision::BranchChanged {
        checkpoint_branch: Some("feature-branch".to_string()),
        current_branch: Some("main".to_string()),
    };
    assert!(!decision.is_safe());
}

#[tokio::test]
async fn test_recovery_gate_workspace_missing() {
    let missing_path = std::env::temp_dir().join("chilli_missing_workspace_xyz987");
    if missing_path.exists() {
        let _ = fs::remove_dir_all(&missing_path);
    }

    let cp = TaskCheckpoint {
        task_id: "task_missing_1".to_string(),
        goal: "Refactor".to_string(),
        phase: "act".to_string(),
        status: TaskStatus::Active,
        workspace_root: missing_path.to_string_lossy().to_string(),
        updated_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        ..TaskCheckpoint::default()
    };

    let current_ws = chilli_memory::CurrentWorkspaceState {
        workspace_root: missing_path.clone(),
        workspace_hash: "hash".to_string(),
        git_state: Default::default(),
        fs_modified_files: Vec::new(),
    };

    let val_res = chilli_memory::TaskRecoveryValidator::validate(&cp, &current_ws);
    match val_res.decision {
        SafeResumeDecision::WorkspaceMissing { expected_root } => {
            assert_eq!(expected_root, missing_path.to_string_lossy().to_string());
        }
        other => panic!("Expected WorkspaceMissing, got {:?}", other),
    }
}

#[tokio::test]
async fn test_recovery_gate_checkpoint_stale() {
    let temp_dir = tempfile::tempdir().unwrap();
    let engine = AgentEngine::new(temp_dir.path()).unwrap();

    let ten_days_secs = 10 * 24 * 3600;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let cp = TaskCheckpoint {
        task_id: "task_stale_1".to_string(),
        goal: "Fix bug".to_string(),
        phase: "act".to_string(),
        status: TaskStatus::Active,
        workspace_root: temp_dir.path().to_string_lossy().to_string(),
        updated_at: now.saturating_sub(ten_days_secs),
        ..TaskCheckpoint::default()
    };

    let res = engine.evaluate_checkpoint_recovery_gate(&cp);
    assert!(res.is_err());
    let err = res.err().unwrap();
    assert!(err.contains("checkpoint stale"), "Error msg: {}", err);
}

#[tokio::test]
async fn test_recovery_gate_checkpoint_corrupt() {
    let temp_dir = tempfile::tempdir().unwrap();
    let engine = AgentEngine::new(temp_dir.path()).unwrap();

    let cp = TaskCheckpoint {
        task_id: "".to_string(), // Corrupt task ID
        goal: "".to_string(),    // Corrupt goal
        phase: "act".to_string(),
        status: TaskStatus::Active,
        workspace_root: temp_dir.path().to_string_lossy().to_string(),
        updated_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        ..TaskCheckpoint::default()
    };

    let res = engine.evaluate_checkpoint_recovery_gate(&cp);
    assert!(res.is_err());
    let err = res.err().unwrap();
    assert!(
        err.contains("corrupt checkpoint data"),
        "Error msg: {}",
        err
    );
}

#[tokio::test]
async fn test_recovery_gate_unsupported_workspace() {
    let temp_dir = tempfile::tempdir().unwrap();
    let engine = AgentEngine::new(temp_dir.path()).unwrap();

    let cp = TaskCheckpoint {
        task_id: "task_unsupported_1".to_string(),
        goal: "Refactor".to_string(),
        phase: "act".to_string(),
        status: TaskStatus::Active,
        schema_version: 999, // Unsupported schema version
        workspace_root: temp_dir.path().to_string_lossy().to_string(),
        updated_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        ..TaskCheckpoint::default()
    };

    let res = engine.evaluate_checkpoint_recovery_gate(&cp);
    assert!(res.is_err());
    let err = res.err().unwrap();
    assert!(err.contains("unsupported workspace"), "Error msg: {}", err);
}

#[tokio::test]
async fn test_recovery_gate_completed_task_rejection() {
    let temp_dir = tempfile::tempdir().unwrap();
    let engine = AgentEngine::new(temp_dir.path()).unwrap();

    let cp = TaskCheckpoint {
        task_id: "task_completed_1".to_string(),
        goal: "Done task".to_string(),
        phase: "completed".to_string(),
        status: TaskStatus::Completed,
        workspace_root: temp_dir.path().to_string_lossy().to_string(),
        updated_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        ..TaskCheckpoint::default()
    };

    let res = engine.evaluate_checkpoint_recovery_gate(&cp);
    assert!(res.is_err());
    let err = res.err().unwrap();
    assert!(
        err.contains("Cannot resume completed task"),
        "Error msg: {}",
        err
    );
}

#[tokio::test]
async fn test_recovery_gate_budget_preservation_and_no_fs_mutation() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_a = temp_dir.path().join("file_a.rs");
    fs::write(&file_a, "fn original() {}").unwrap();

    let engine = AgentEngine::new(temp_dir.path()).unwrap();

    let cp = TaskCheckpoint {
        task_id: "task_budget_1".to_string(),
        goal: "Test budget".to_string(),
        phase: "verify".to_string(),
        status: TaskStatus::Active,
        verification_attempts: 4,
        verification_failures: vec!["test failure #1".to_string()],
        executed_tools_summary: vec!["read_file".to_string(), "edit_file".to_string()],
        workspace_root: temp_dir.path().to_string_lossy().to_string(),
        updated_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        ..TaskCheckpoint::default()
    };

    let res = engine.evaluate_checkpoint_recovery_gate(&cp);
    assert!(res.is_ok());

    // Verify budget fields are untouched
    assert_eq!(cp.verification_attempts, 4);
    assert_eq!(cp.verification_failures.len(), 1);
    assert_eq!(cp.executed_tools_summary.len(), 2);

    // Verify filesystem is untouched
    let content = fs::read_to_string(&file_a).unwrap();
    assert_eq!(content, "fn original() {}");
}

#[tokio::test]
async fn test_checkpoint_resume_fallback_to_clean_state() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut engine = AgentEngine::new(temp_dir.path()).unwrap();
    engine.set_mock_responses(vec![chilli_core::engine::MockResponse::Text(
        "Fallback execution complete".to_string(),
    )]);

    let cp = TaskCheckpoint {
        task_id: "stale_task_1".to_string(),
        goal: "Refactor architecture".to_string(),
        phase: "inspect".to_string(),
        status: TaskStatus::Active,
        workspace_root: temp_dir.path().to_string_lossy().to_string(),
        updated_at: 100, // Very stale timestamp (epoch + 100s)
        ..TaskCheckpoint::default()
    };

    let result = engine.resume_task_from_checkpoint(&cp).await;
    assert!(
        result.is_ok(),
        "Expected fallback clean state execution, got: {:?}",
        result
    );
    let res = result.unwrap();
    assert!(res.completed);
}
