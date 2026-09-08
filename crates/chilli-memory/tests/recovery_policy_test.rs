use chilli_memory::{
    CompactTaskTelemetry, CurrentWorkspaceState, GitWorkspaceState, SafeResumeDecision,
    TaskCheckpoint, TaskRecoveryValidator, TaskStatus, CURRENT_SCHEMA_VERSION,
};
use tempfile::tempdir;

use std::time::{SystemTime, UNIX_EPOCH};

fn sample_checkpoint(id: &str, status: TaskStatus) -> TaskCheckpoint {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    TaskCheckpoint {
        task_id: id.to_string(),
        workspace_root: "/test/workspace".to_string(),
        workspace_hash: "hash123".to_string(),
        git_commit: Some("abc123def456".to_string()),
        goal: "Refactor task persistence".to_string(),
        phase: "implementation".to_string(),
        status,
        execution_strategy: Some("branch:main".to_string()),
        completed_subtasks: vec!["setup storage".to_string()],
        pending_subtasks: vec!["write tests".to_string()],
        modified_files: vec!["src/task_store.rs".to_string()],
        visited_files: vec!["src/storage.rs".to_string(), "src/lib.rs".to_string()],
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
        created_at: now,
        updated_at: now,
    }
}

#[test]
fn test_case_a_exact_unchanged_workspace() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let cp = sample_checkpoint("task-a", TaskStatus::Active);

    let workspace = CurrentWorkspaceState {
        workspace_root: root,
        workspace_hash: "hash123".to_string(),
        git_state: GitWorkspaceState {
            is_git_repo: true,
            current_branch: Some("main".to_string()),
            current_commit: Some("abc123def456".to_string()),
            is_dirty: false,
            untracked_files: vec![],
            modified_files: vec![],
        },
        fs_modified_files: vec![],
    };

    let result = TaskRecoveryValidator::validate(&cp, &workspace);
    assert_eq!(result.decision, SafeResumeDecision::SafeToResume);
    assert!(result.decision.is_safe());
    assert!(result.decision.can_resume_interactively());
    assert_eq!(result.telemetry.decision_tag, "safe_to_resume");
}

#[test]
fn test_case_b_chilli_modified_files_only() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let cp = sample_checkpoint("task-b", TaskStatus::Active);

    let workspace = CurrentWorkspaceState {
        workspace_root: root,
        workspace_hash: "hash123".to_string(),
        git_state: GitWorkspaceState {
            is_git_repo: true,
            current_branch: Some("main".to_string()),
            current_commit: Some("abc123def456".to_string()),
            is_dirty: true,
            untracked_files: vec![],
            modified_files: vec!["src/task_store.rs".to_string()],
        },
        fs_modified_files: vec!["src/task_store.rs".to_string()],
    };

    let result = TaskRecoveryValidator::validate(&cp, &workspace);
    assert_eq!(result.decision, SafeResumeDecision::SafeToResume);
    assert_eq!(result.expected_modifications, vec!["src/task_store.rs"]);
    assert!(result.external_modifications.is_empty());
}

#[test]
fn test_case_c_unrelated_external_modification() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let cp = sample_checkpoint("task-c", TaskStatus::Active);

    let workspace = CurrentWorkspaceState {
        workspace_root: root,
        workspace_hash: "hash123".to_string(),
        git_state: GitWorkspaceState {
            is_git_repo: true,
            current_branch: Some("main".to_string()),
            current_commit: Some("abc123def456".to_string()),
            is_dirty: true,
            untracked_files: vec![],
            modified_files: vec![
                "src/task_store.rs".to_string(),
                "docs/readme.md".to_string(),
            ],
        },
        fs_modified_files: vec![
            "src/task_store.rs".to_string(),
            "docs/readme.md".to_string(),
        ],
    };

    let result = TaskRecoveryValidator::validate(&cp, &workspace);
    assert!(matches!(
        result.decision,
        SafeResumeDecision::SafeWithNotice { .. }
    ));
    assert!(result.decision.is_safe());
    assert_eq!(result.expected_modifications, vec!["src/task_store.rs"]);
    assert_eq!(result.external_modifications, vec!["docs/readme.md"]);
    assert!(result.conflicting_modifications.is_empty());
}

#[test]
fn test_case_d_overlapping_external_modification() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let cp = sample_checkpoint("task-d", TaskStatus::Active);

    let workspace = CurrentWorkspaceState {
        workspace_root: root,
        workspace_hash: "hash123".to_string(),
        git_state: GitWorkspaceState {
            is_git_repo: true,
            current_branch: Some("main".to_string()),
            current_commit: Some("abc123def456".to_string()),
            is_dirty: true,
            untracked_files: vec![],
            modified_files: vec!["src/storage.rs".to_string()],
        },
        fs_modified_files: vec!["src/storage.rs".to_string()],
    };

    let result = TaskRecoveryValidator::validate(&cp, &workspace);
    assert!(matches!(
        result.decision,
        SafeResumeDecision::ExternalChangesDetected { .. }
    ));
    assert!(!result.decision.is_safe());
    assert!(result.decision.can_resume_interactively());
    assert_eq!(result.conflicting_modifications, vec!["src/storage.rs"]);
}

#[test]
fn test_case_e_commit_advanced_safely() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let cp = sample_checkpoint("task-e", TaskStatus::Active);

    let workspace = CurrentWorkspaceState {
        workspace_root: root,
        workspace_hash: "hash123".to_string(),
        git_state: GitWorkspaceState {
            is_git_repo: true,
            current_branch: Some("main".to_string()),
            current_commit: Some("999999ffffffff".to_string()),
            is_dirty: false,
            untracked_files: vec![],
            modified_files: vec![],
        },
        fs_modified_files: vec![],
    };

    let result = TaskRecoveryValidator::validate(&cp, &workspace);
    assert!(matches!(
        result.decision,
        SafeResumeDecision::SafeWithNotice { .. }
    ));
    assert!(result.decision.is_safe());
}

#[test]
fn test_case_f_branch_changed() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let cp = sample_checkpoint("task-f", TaskStatus::Active);

    let workspace = CurrentWorkspaceState {
        workspace_root: root,
        workspace_hash: "hash123".to_string(),
        git_state: GitWorkspaceState {
            is_git_repo: true,
            current_branch: Some("feature/other".to_string()),
            current_commit: Some("abc123def456".to_string()),
            is_dirty: false,
            untracked_files: vec![],
            modified_files: vec![],
        },
        fs_modified_files: vec![],
    };

    let result = TaskRecoveryValidator::validate(&cp, &workspace);
    assert_eq!(
        result.decision,
        SafeResumeDecision::BranchChanged {
            checkpoint_branch: Some("main".to_string()),
            current_branch: Some("feature/other".to_string()),
        }
    );
    assert!(!result.decision.is_safe());
}

#[test]
fn test_case_g_git_repo_missing() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let cp = sample_checkpoint("task-g", TaskStatus::Active);

    let workspace = CurrentWorkspaceState {
        workspace_root: root,
        workspace_hash: "hash123".to_string(),
        git_state: GitWorkspaceState {
            is_git_repo: false,
            current_branch: None,
            current_commit: None,
            is_dirty: false,
            untracked_files: vec![],
            modified_files: vec![],
        },
        fs_modified_files: vec![],
    };

    let result = TaskRecoveryValidator::validate(&cp, &workspace);
    assert!(matches!(
        result.decision,
        SafeResumeDecision::SafeWithNotice { .. }
    ));
    assert!(result.decision.is_safe());
}

#[test]
fn test_case_h_workspace_missing() {
    let root = tempdir().unwrap().path().join("non_existent_folder");
    let cp = sample_checkpoint("task-h", TaskStatus::Active);

    let workspace = CurrentWorkspaceState {
        workspace_root: root,
        workspace_hash: "hash123".to_string(),
        git_state: GitWorkspaceState::default(),
        fs_modified_files: vec![],
    };

    let result = TaskRecoveryValidator::validate(&cp, &workspace);
    assert!(matches!(
        result.decision,
        SafeResumeDecision::WorkspaceMissing { .. }
    ));
    assert!(!result.decision.is_safe());
}

#[test]
fn test_case_j_corrupt_or_unrecoverable_status() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let cp = sample_checkpoint("task-j", TaskStatus::Completed);

    let workspace = CurrentWorkspaceState {
        workspace_root: root,
        workspace_hash: "hash123".to_string(),
        git_state: GitWorkspaceState::default(),
        fs_modified_files: vec![],
    };

    let result = TaskRecoveryValidator::validate(&cp, &workspace);
    assert!(matches!(
        result.decision,
        SafeResumeDecision::UnsafeToResume { .. }
    ));
    assert!(!result.decision.is_safe());
}

#[test]
fn test_case_k_unsupported_schema_version() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let mut cp = sample_checkpoint("task-k", TaskStatus::Active);
    cp.schema_version = CURRENT_SCHEMA_VERSION + 5;

    let workspace = CurrentWorkspaceState {
        workspace_root: root,
        workspace_hash: "hash123".to_string(),
        git_state: GitWorkspaceState::default(),
        fs_modified_files: vec![],
    };

    let result = TaskRecoveryValidator::validate(&cp, &workspace);
    assert!(matches!(
        result.decision,
        SafeResumeDecision::UnsupportedWorkspace { .. }
    ));
    assert!(!result.decision.is_safe());
}

#[test]
fn test_case_n_credential_filenames_filtered() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let cp = sample_checkpoint("task-n", TaskStatus::Active);

    let workspace = CurrentWorkspaceState {
        workspace_root: root,
        workspace_hash: "hash123".to_string(),
        git_state: GitWorkspaceState {
            is_git_repo: true,
            current_branch: Some("main".to_string()),
            current_commit: Some("abc123def456".to_string()),
            is_dirty: true,
            untracked_files: vec![".env".to_string(), ".env.local".to_string()],
            modified_files: vec!["src/task_store.rs".to_string()],
        },
        fs_modified_files: vec![
            "src/task_store.rs".to_string(),
            ".env".to_string(),
            ".env.local".to_string(),
        ],
    };

    let result = TaskRecoveryValidator::validate(&cp, &workspace);
    assert_eq!(result.decision, SafeResumeDecision::SafeToResume);
    assert_eq!(result.expected_modifications, vec!["src/task_store.rs"]);
    assert!(result.external_modifications.is_empty());
}
