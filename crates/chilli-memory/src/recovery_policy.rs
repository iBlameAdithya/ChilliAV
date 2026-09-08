use crate::task_store::{TaskCheckpoint, TaskSanitizer, CURRENT_SCHEMA_VERSION};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct GitWorkspaceState {
    pub is_git_repo: bool,
    pub current_branch: Option<String>,
    pub current_commit: Option<String>,
    pub is_dirty: bool,
    pub untracked_files: Vec<String>,
    pub modified_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CurrentWorkspaceState {
    pub workspace_root: PathBuf,
    pub workspace_hash: String,
    pub git_state: GitWorkspaceState,
    pub fs_modified_files: Vec<String>,
}

impl CurrentWorkspaceState {
    pub fn compute_workspace_hash<P: AsRef<Path>>(root: P) -> String {
        let canonical = root
            .as_ref()
            .canonicalize()
            .unwrap_or_else(|_| root.as_ref().to_path_buf());
        let path_str = canonical.to_string_lossy();
        format!("{:x}", fnv1a_hash_64(path_str.as_bytes()))
    }

    pub fn inspect<P: AsRef<Path>>(workspace_root: P) -> Self {
        let root = workspace_root.as_ref().to_path_buf();
        let hash = Self::compute_workspace_hash(&root);
        let git_state = Self::inspect_git(&root);

        let mut fs_modified = git_state.modified_files.clone();
        for u in &git_state.untracked_files {
            if !fs_modified.contains(u) {
                fs_modified.push(u.clone());
            }
        }

        Self {
            workspace_root: root,
            workspace_hash: hash,
            git_state,
            fs_modified_files: fs_modified,
        }
    }

    fn inspect_git(root: &Path) -> GitWorkspaceState {
        let git_dir = root.join(".git");
        if !git_dir.exists() {
            return GitWorkspaceState::default();
        }

        let branch_out = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(root)
            .output();

        let current_branch = match branch_out {
            Ok(out) if out.status.success() => {
                let b = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if b.is_empty() || b == "HEAD" {
                    None
                } else {
                    Some(b)
                }
            }
            _ => None,
        };

        let commit_out = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(root)
            .output();

        let current_commit = match commit_out {
            Ok(out) if out.status.success() => {
                let c = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if c.is_empty() {
                    None
                } else {
                    Some(c)
                }
            }
            _ => None,
        };

        let status_out = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(root)
            .output();

        let mut modified_files = Vec::new();
        let mut untracked_files = Vec::new();

        if let Ok(out) = status_out {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                for line in text.lines() {
                    if line.len() < 4 {
                        continue;
                    }
                    let status_code = &line[0..2];
                    let file_path = line[3..].trim().to_string();
                    if TaskSanitizer::is_sensitive_file(&file_path) {
                        continue;
                    }
                    if status_code == "??" {
                        untracked_files.push(file_path);
                    } else {
                        modified_files.push(file_path);
                    }
                }
            }
        }

        let is_dirty = !modified_files.is_empty() || !untracked_files.is_empty();

        GitWorkspaceState {
            is_git_repo: true,
            current_branch,
            current_commit,
            is_dirty,
            untracked_files,
            modified_files,
        }
    }
}

fn fnv1a_hash_64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SafeResumeDecision {
    SafeToResume,
    SafeWithNotice {
        notices: Vec<String>,
    },
    ExternalChangesDetected {
        conflicting_files: Vec<String>,
        non_conflicting_files: Vec<String>,
    },
    BranchChanged {
        checkpoint_branch: Option<String>,
        current_branch: Option<String>,
    },
    WorkspaceMissing {
        expected_root: String,
    },
    RepositoryChanged {
        checkpoint_commit: Option<String>,
        current_commit: Option<String>,
    },
    CheckpointStale {
        age_secs: u64,
        max_allowed_secs: u64,
    },
    CheckpointCorrupt {
        details: String,
    },
    UnsupportedWorkspace {
        reason: String,
    },
    UnsafeToResume {
        reasons: Vec<String>,
    },
}

impl SafeResumeDecision {
    pub fn is_safe(&self) -> bool {
        matches!(
            self,
            SafeResumeDecision::SafeToResume | SafeResumeDecision::SafeWithNotice { .. }
        )
    }

    pub fn can_resume_interactively(&self) -> bool {
        matches!(
            self,
            SafeResumeDecision::SafeToResume
                | SafeResumeDecision::SafeWithNotice { .. }
                | SafeResumeDecision::ExternalChangesDetected { .. }
                | SafeResumeDecision::RepositoryChanged { .. }
        )
    }

    pub fn category(&self) -> &'static str {
        match self {
            SafeResumeDecision::SafeToResume => "safe_to_resume",
            SafeResumeDecision::SafeWithNotice { .. } => "safe_with_notice",
            SafeResumeDecision::ExternalChangesDetected { .. } => "external_changes_detected",
            SafeResumeDecision::BranchChanged { .. } => "branch_changed",
            SafeResumeDecision::WorkspaceMissing { .. } => "workspace_missing",
            SafeResumeDecision::RepositoryChanged { .. } => "repository_changed",
            SafeResumeDecision::CheckpointStale { .. } => "checkpoint_stale",
            SafeResumeDecision::CheckpointCorrupt { .. } => "checkpoint_corrupt",
            SafeResumeDecision::UnsupportedWorkspace { .. } => "unsupported_workspace",
            SafeResumeDecision::UnsafeToResume { .. } => "unsafe_to_resume",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationTelemetry {
    pub validation_duration_ms: u64,
    pub changed_files_count: usize,
    pub conflicting_files_count: usize,
    pub checkpoint_age_secs: u64,
    pub decision_tag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceValidationResult {
    pub decision: SafeResumeDecision,
    pub expected_modifications: Vec<String>,
    pub external_modifications: Vec<String>,
    pub conflicting_modifications: Vec<String>,
    pub git_state: GitWorkspaceState,
    pub telemetry: ValidationTelemetry,
}

pub struct TaskRecoveryValidator;

impl TaskRecoveryValidator {
    pub const DEFAULT_MAX_CHECKPOINT_AGE_SECS: u64 = 7 * 24 * 3600; // 7 days

    pub fn validate(
        checkpoint: &TaskCheckpoint,
        current_workspace: &CurrentWorkspaceState,
    ) -> WorkspaceValidationResult {
        Self::validate_with_max_age(
            checkpoint,
            current_workspace,
            Self::DEFAULT_MAX_CHECKPOINT_AGE_SECS,
        )
    }

    pub fn validate_with_max_age(
        checkpoint: &TaskCheckpoint,
        current_workspace: &CurrentWorkspaceState,
        max_age_secs: u64,
    ) -> WorkspaceValidationResult {
        let start = Instant::now();

        let mut expected_modifications = Vec::new();
        let mut external_modifications = Vec::new();
        let mut conflicting_modifications = Vec::new();
        let mut notices = Vec::new();
        let unsafe_reasons = Vec::new();

        // 0. Corruption check (fail-closed)
        if checkpoint.task_id.trim().is_empty() || checkpoint.goal.trim().is_empty() {
            return Self::build_result(
                SafeResumeDecision::CheckpointCorrupt {
                    details: "Checkpoint task_id or goal is empty".to_string(),
                },
                expected_modifications,
                external_modifications,
                conflicting_modifications,
                current_workspace.git_state.clone(),
                0,
                start,
            );
        }

        // 1. Schema version check (fail-closed)
        if checkpoint.schema_version > CURRENT_SCHEMA_VERSION {
            return Self::build_result(
                SafeResumeDecision::UnsupportedWorkspace {
                    reason: format!(
                        "Checkpoint schema version {} exceeds supported current version {}",
                        checkpoint.schema_version, CURRENT_SCHEMA_VERSION
                    ),
                },
                expected_modifications,
                external_modifications,
                conflicting_modifications,
                current_workspace.git_state.clone(),
                0,
                start,
            );
        }

        // 2. Task status check
        if !checkpoint.status.is_recoverable_candidate() {
            return Self::build_result(
                SafeResumeDecision::UnsafeToResume {
                    reasons: vec![format!(
                        "Task status '{:?}' is not recoverable",
                        checkpoint.status
                    )],
                },
                expected_modifications,
                external_modifications,
                conflicting_modifications,
                current_workspace.git_state.clone(),
                0,
                start,
            );
        }

        // 3. Workspace root existence check
        if !current_workspace.workspace_root.exists() {
            return Self::build_result(
                SafeResumeDecision::WorkspaceMissing {
                    expected_root: checkpoint.workspace_root.clone(),
                },
                expected_modifications,
                external_modifications,
                conflicting_modifications,
                current_workspace.git_state.clone(),
                0,
                start,
            );
        }

        // 4. Age check
        let now_ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let checkpoint_age = now_ts.saturating_sub(checkpoint.updated_at);

        if max_age_secs > 0 && checkpoint_age > max_age_secs {
            return Self::build_result(
                SafeResumeDecision::CheckpointStale {
                    age_secs: checkpoint_age,
                    max_allowed_secs: max_age_secs,
                },
                expected_modifications,
                external_modifications,
                conflicting_modifications,
                current_workspace.git_state.clone(),
                checkpoint_age,
                start,
            );
        }

        // 5. Git / Branch & Commit validation
        if current_workspace.git_state.is_git_repo {
            // Check branch change
            if let (Some(ref cp_branch), Some(ref cur_branch)) = (
                &Self::extract_branch_from_checkpoint(checkpoint),
                &current_workspace.git_state.current_branch,
            ) {
                if cp_branch != cur_branch {
                    return Self::build_result(
                        SafeResumeDecision::BranchChanged {
                            checkpoint_branch: Some(cp_branch.clone()),
                            current_branch: Some(cur_branch.clone()),
                        },
                        expected_modifications,
                        external_modifications,
                        conflicting_modifications,
                        current_workspace.git_state.clone(),
                        checkpoint_age,
                        start,
                    );
                }
            }

            // Check commit divergence
            if let (Some(ref cp_commit), Some(ref cur_commit)) = (
                &checkpoint.git_commit,
                &current_workspace.git_state.current_commit,
            ) {
                if cp_commit != cur_commit {
                    notices.push(format!(
                        "Git commit advanced from {} to {}",
                        &cp_commit[..cp_commit.len().min(7)],
                        &cur_commit[..cur_commit.len().min(7)]
                    ));
                }
            }
        } else if checkpoint.git_commit.is_some() {
            notices.push(
                "Checkpoint had Git commit metadata, but current workspace is non-git".to_string(),
            );
        }

        // 6. Expected vs External file modifications
        let checkpoint_modified_set: HashSet<&str> = checkpoint
            .modified_files
            .iter()
            .map(|s| s.as_str())
            .collect();
        let checkpoint_visited_set: HashSet<&str> = checkpoint
            .visited_files
            .iter()
            .map(|s| s.as_str())
            .collect();

        for current_file in &current_workspace.fs_modified_files {
            if TaskSanitizer::is_sensitive_file(current_file) {
                continue;
            }

            if checkpoint_modified_set.contains(current_file.as_str()) {
                expected_modifications.push(current_file.clone());
            } else {
                external_modifications.push(current_file.clone());
                // Conflict if external change is in files visited or targeted by Chilli
                if checkpoint_visited_set.contains(current_file.as_str()) {
                    conflicting_modifications.push(current_file.clone());
                }
            }
        }

        // 7. Decision determination
        let decision = if !conflicting_modifications.is_empty() {
            SafeResumeDecision::ExternalChangesDetected {
                conflicting_files: conflicting_modifications.clone(),
                non_conflicting_files: external_modifications
                    .iter()
                    .filter(|f| !conflicting_modifications.contains(f))
                    .cloned()
                    .collect(),
            }
        } else if !external_modifications.is_empty() || !notices.is_empty() {
            if !external_modifications.is_empty() {
                notices.push(format!(
                    "{} unrelated external file modifications detected",
                    external_modifications.len()
                ));
            }
            SafeResumeDecision::SafeWithNotice { notices }
        } else if !unsafe_reasons.is_empty() {
            SafeResumeDecision::UnsafeToResume {
                reasons: unsafe_reasons,
            }
        } else {
            SafeResumeDecision::SafeToResume
        };

        Self::build_result(
            decision,
            expected_modifications,
            external_modifications,
            conflicting_modifications,
            current_workspace.git_state.clone(),
            checkpoint_age,
            start,
        )
    }

    fn extract_branch_from_checkpoint(checkpoint: &TaskCheckpoint) -> Option<String> {
        // Option to store branch in execution_strategy or phase if recorded
        if let Some(ref strat) = checkpoint.execution_strategy {
            if strat.starts_with("branch:") {
                return Some(strat.trim_start_matches("branch:").to_string());
            }
        }
        None
    }

    fn build_result(
        decision: SafeResumeDecision,
        expected_modifications: Vec<String>,
        external_modifications: Vec<String>,
        conflicting_modifications: Vec<String>,
        git_state: GitWorkspaceState,
        checkpoint_age_secs: u64,
        start: Instant,
    ) -> WorkspaceValidationResult {
        let duration_ms = start.elapsed().as_millis() as u64;
        let decision_tag = decision.category().to_string();
        let changed_files_count = expected_modifications.len() + external_modifications.len();
        let conflicting_files_count = conflicting_modifications.len();

        WorkspaceValidationResult {
            decision,
            expected_modifications,
            external_modifications,
            conflicting_modifications,
            git_state,
            telemetry: ValidationTelemetry {
                validation_duration_ms: duration_ms,
                changed_files_count,
                conflicting_files_count,
                checkpoint_age_secs,
                decision_tag,
            },
        }
    }
}
