use crate::storage::StoragePaths;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Error, Debug)]
pub enum TaskStoreError {
    #[error("Database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Mutex lock error: {0}")]
    Lock(String),
    #[error("Storage error: {0}")]
    Storage(#[from] crate::storage::StorageError),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Unknown task status: '{0}'")]
    UnknownStatus(String),
    #[error("Unsupported schema version: stored {stored}, current {current}")]
    UnsupportedSchemaVersion { stored: u32, current: u32 },
    #[error("Corrupted checkpoint data for task '{task_id}': {details}")]
    CorruptedData { task_id: String, details: String },
    #[error("Task checkpoint not found: '{0}'")]
    NotFound(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Active,
    Paused,
    Interrupted,
    Recoverable,
    Completed,
    Failed,
    Abandoned,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Active => "active",
            TaskStatus::Paused => "paused",
            TaskStatus::Interrupted => "interrupted",
            TaskStatus::Recoverable => "recoverable",
            TaskStatus::Completed => "completed",
            TaskStatus::Failed => "failed",
            TaskStatus::Abandoned => "abandoned",
        }
    }

    pub fn parse(s: &str) -> Result<Self, TaskStoreError> {
        match s.trim().to_lowercase().as_str() {
            "active" => Ok(TaskStatus::Active),
            "paused" => Ok(TaskStatus::Paused),
            "interrupted" => Ok(TaskStatus::Interrupted),
            "recoverable" => Ok(TaskStatus::Recoverable),
            "completed" => Ok(TaskStatus::Completed),
            "failed" => Ok(TaskStatus::Failed),
            "abandoned" => Ok(TaskStatus::Abandoned),
            _ => Err(TaskStoreError::UnknownStatus(s.to_string())),
        }
    }

    pub fn is_recoverable_candidate(&self) -> bool {
        matches!(
            self,
            TaskStatus::Active
                | TaskStatus::Paused
                | TaskStatus::Interrupted
                | TaskStatus::Recoverable
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CompactTaskTelemetry {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cached_tokens: usize,
    pub total_tokens: usize,
    pub total_turns: usize,
    pub tool_calls_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskCheckpoint {
    pub task_id: String,
    pub workspace_root: String,
    pub workspace_hash: String,
    pub git_commit: Option<String>,
    pub goal: String,
    pub phase: String,
    pub status: TaskStatus,
    pub execution_strategy: Option<String>,
    pub completed_subtasks: Vec<String>,
    pub pending_subtasks: Vec<String>,
    pub modified_files: Vec<String>,
    pub visited_files: Vec<String>,
    pub verification_attempts: usize,
    pub verification_failures: Vec<String>,
    pub executed_tools_summary: Vec<String>,
    pub telemetry: CompactTaskTelemetry,
    pub schema_version: u32,
    pub created_at: u64,
    pub updated_at: u64,
}

impl TaskCheckpoint {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Abandoned
        )
    }

    pub fn subtask_completion_rate(&self) -> f64 {
        let total = self.completed_subtasks.len() + self.pending_subtasks.len();
        if total == 0 {
            0.0
        } else {
            self.completed_subtasks.len() as f64 / total as f64
        }
    }
}

impl Default for TaskCheckpoint {
    fn default() -> Self {
        Self {
            task_id: "default_task".to_string(),
            workspace_root: ".".to_string(),
            workspace_hash: "default_hash".to_string(),
            git_commit: None,
            goal: "Default Goal".to_string(),
            phase: "understand".to_string(),
            status: TaskStatus::Active,
            execution_strategy: None,
            completed_subtasks: Vec::new(),
            pending_subtasks: Vec::new(),
            modified_files: Vec::new(),
            visited_files: Vec::new(),
            verification_attempts: 0,
            verification_failures: Vec::new(),
            executed_tools_summary: Vec::new(),
            telemetry: CompactTaskTelemetry::default(),
            schema_version: CURRENT_SCHEMA_VERSION,
            created_at: 0,
            updated_at: 0,
        }
    }
}

pub struct TaskSanitizer;

impl TaskSanitizer {
    pub fn is_sensitive_file(path_str: &str) -> bool {
        let p_lower = path_str.trim().to_lowercase();
        let sensitive_patterns = [
            ".env",
            ".ssh",
            ".aws",
            ".gcp",
            ".azure",
            ".kdbx",
            "id_rsa",
            "id_ed25519",
            "chrome/user data",
            "chrome\\user data",
        ];
        for pattern in &sensitive_patterns {
            if p_lower.contains(pattern) {
                return true;
            }
        }
        false
    }

    pub fn sanitize_text(text: &str) -> String {
        let mut cleaned = text.to_string();
        let excluded_patterns = [
            "aws_secret",
            "aws_access_key",
            "bearer ",
            "private_key",
            "api_key",
            "password",
            "secret_key",
        ];

        for pattern in &excluded_patterns {
            if cleaned.to_lowercase().contains(pattern) {
                cleaned = format!("[REDACTED_CREDENTIAL: contained {}]", pattern);
            }
        }
        cleaned
    }

    pub fn sanitize(checkpoint: &mut TaskCheckpoint) {
        checkpoint
            .modified_files
            .retain(|f| !Self::is_sensitive_file(f));
        checkpoint
            .visited_files
            .retain(|f| !Self::is_sensitive_file(f));

        checkpoint.goal = Self::sanitize_text(&checkpoint.goal);
        checkpoint.completed_subtasks = checkpoint
            .completed_subtasks
            .iter()
            .map(|s| Self::sanitize_text(s))
            .collect();
        checkpoint.pending_subtasks = checkpoint
            .pending_subtasks
            .iter()
            .map(|s| Self::sanitize_text(s))
            .collect();
        checkpoint.verification_failures = checkpoint
            .verification_failures
            .iter()
            .map(|s| Self::sanitize_text(s))
            .collect();
        checkpoint.executed_tools_summary = checkpoint
            .executed_tools_summary
            .iter()
            .map(|s| Self::sanitize_text(s))
            .collect();
    }
}

pub struct TaskCheckpointStore {
    conn: Mutex<Connection>,
}

impl TaskCheckpointStore {
    pub fn new<P: AsRef<Path>>(workspace_root: P) -> Result<Self, TaskStoreError> {
        let paths = StoragePaths::resolve(workspace_root)?;
        Self::with_custom_path(&paths.task_store_db_path)
    }

    pub fn with_custom_path<P: AsRef<Path>>(db_path: P) -> Result<Self, TaskStoreError> {
        let path = db_path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let conn = Self::init_db(path)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn init_db(path: &Path) -> Result<Connection, TaskStoreError> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;

        // Check stored schema version using pragma user_version
        let stored_version: u32 = conn.query_row("PRAGMA user_version;", [], |row| row.get(0))?;

        if stored_version > CURRENT_SCHEMA_VERSION {
            return Err(TaskStoreError::UnsupportedSchemaVersion {
                stored: stored_version,
                current: CURRENT_SCHEMA_VERSION,
            });
        }

        conn.execute(
            "CREATE TABLE IF NOT EXISTS task_checkpoints (
                task_id TEXT PRIMARY KEY,
                workspace_root TEXT NOT NULL,
                workspace_hash TEXT NOT NULL,
                git_commit TEXT,
                goal TEXT NOT NULL,
                phase TEXT NOT NULL,
                status TEXT NOT NULL,
                execution_strategy TEXT,
                completed_subtasks_json TEXT NOT NULL,
                pending_subtasks_json TEXT NOT NULL,
                modified_files_json TEXT NOT NULL,
                visited_files_json TEXT NOT NULL,
                verification_attempts INTEGER NOT NULL DEFAULT 0,
                verification_failures_json TEXT NOT NULL,
                executed_tools_summary_json TEXT NOT NULL,
                telemetry_json TEXT NOT NULL,
                schema_version INTEGER NOT NULL DEFAULT 1,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_task_checkpoints_status ON task_checkpoints(status);",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_task_checkpoints_updated ON task_checkpoints(updated_at);",
            [],
        )?;

        if stored_version < CURRENT_SCHEMA_VERSION {
            conn.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)?;
        }

        Ok(conn)
    }

    fn now_ts() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    pub fn save_checkpoint(&self, checkpoint: &TaskCheckpoint) -> Result<(), TaskStoreError> {
        let mut cp = checkpoint.clone();
        TaskSanitizer::sanitize(&mut cp);

        if cp.schema_version > CURRENT_SCHEMA_VERSION {
            return Err(TaskStoreError::UnsupportedSchemaVersion {
                stored: cp.schema_version,
                current: CURRENT_SCHEMA_VERSION,
            });
        }

        let mut conn = self
            .conn
            .lock()
            .map_err(|e| TaskStoreError::Lock(e.to_string()))?;

        let tx = conn.transaction()?;

        let now = Self::now_ts();
        let created_at = if cp.created_at == 0 {
            now
        } else {
            cp.created_at
        };
        let updated_at = now;

        let completed_subtasks_json = serde_json::to_string(&cp.completed_subtasks)?;
        let pending_subtasks_json = serde_json::to_string(&cp.pending_subtasks)?;
        let modified_files_json = serde_json::to_string(&cp.modified_files)?;
        let visited_files_json = serde_json::to_string(&cp.visited_files)?;
        let verification_failures_json = serde_json::to_string(&cp.verification_failures)?;
        let executed_tools_summary_json = serde_json::to_string(&cp.executed_tools_summary)?;
        let telemetry_json = serde_json::to_string(&cp.telemetry)?;

        tx.execute(
            "INSERT INTO task_checkpoints (
                task_id, workspace_root, workspace_hash, git_commit, goal, phase, status,
                execution_strategy, completed_subtasks_json, pending_subtasks_json,
                modified_files_json, visited_files_json, verification_attempts,
                verification_failures_json, executed_tools_summary_json, telemetry_json,
                schema_version, created_at, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19
            ) ON CONFLICT(task_id) DO UPDATE SET
                workspace_root = excluded.workspace_root,
                workspace_hash = excluded.workspace_hash,
                git_commit = excluded.git_commit,
                goal = excluded.goal,
                phase = excluded.phase,
                status = excluded.status,
                execution_strategy = excluded.execution_strategy,
                completed_subtasks_json = excluded.completed_subtasks_json,
                pending_subtasks_json = excluded.pending_subtasks_json,
                modified_files_json = excluded.modified_files_json,
                visited_files_json = excluded.visited_files_json,
                verification_attempts = excluded.verification_attempts,
                verification_failures_json = excluded.verification_failures_json,
                executed_tools_summary_json = excluded.executed_tools_summary_json,
                telemetry_json = excluded.telemetry_json,
                schema_version = excluded.schema_version,
                updated_at = excluded.updated_at;",
            params![
                cp.task_id,
                cp.workspace_root,
                cp.workspace_hash,
                cp.git_commit,
                cp.goal,
                cp.phase,
                cp.status.as_str(),
                cp.execution_strategy,
                completed_subtasks_json,
                pending_subtasks_json,
                modified_files_json,
                visited_files_json,
                cp.verification_attempts as i64,
                verification_failures_json,
                executed_tools_summary_json,
                telemetry_json,
                cp.schema_version as i64,
                created_at as i64,
                updated_at as i64,
            ],
        )?;

        tx.commit()?;
        Ok(())
    }

    pub fn load_checkpoint(&self, task_id: &str) -> Result<TaskCheckpoint, TaskStoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| TaskStoreError::Lock(e.to_string()))?;

        let mut stmt = conn.prepare(
            "SELECT task_id, workspace_root, workspace_hash, git_commit, goal, phase, status,
                    execution_strategy, completed_subtasks_json, pending_subtasks_json,
                    modified_files_json, visited_files_json, verification_attempts,
                    verification_failures_json, executed_tools_summary_json, telemetry_json,
                    schema_version, created_at, updated_at
             FROM task_checkpoints WHERE task_id = ?1 LIMIT 1;",
        )?;

        let checkpoint = stmt
            .query_row(params![task_id], |row| {
                let schema_ver: i64 = row.get(16)?;
                Ok((
                    row.get::<_, String>(0)?,         // task_id
                    row.get::<_, String>(1)?,         // workspace_root
                    row.get::<_, String>(2)?,         // workspace_hash
                    row.get::<_, Option<String>>(3)?, // git_commit
                    row.get::<_, String>(4)?,         // goal
                    row.get::<_, String>(5)?,         // phase
                    row.get::<_, String>(6)?,         // status_str
                    row.get::<_, Option<String>>(7)?, // execution_strategy
                    row.get::<_, String>(8)?,         // completed_subtasks_json
                    row.get::<_, String>(9)?,         // pending_subtasks_json
                    row.get::<_, String>(10)?,        // modified_files_json
                    row.get::<_, String>(11)?,        // visited_files_json
                    row.get::<_, i64>(12)?,           // verification_attempts
                    row.get::<_, String>(13)?,        // verification_failures_json
                    row.get::<_, String>(14)?,        // executed_tools_summary_json
                    row.get::<_, String>(15)?,        // telemetry_json
                    schema_ver as u32,
                    row.get::<_, i64>(17)? as u64,
                    row.get::<_, i64>(18)? as u64,
                ))
            })
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    TaskStoreError::NotFound(task_id.to_string())
                }
                other => TaskStoreError::Sqlite(other),
            })?;

        Self::parse_row_tuple(checkpoint)
    }

    pub fn list_checkpoints(
        &self,
        status_filter: Option<TaskStatus>,
    ) -> Result<Vec<TaskCheckpoint>, TaskStoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| TaskStoreError::Lock(e.to_string()))?;

        let mut sql = "SELECT task_id, workspace_root, workspace_hash, git_commit, goal, phase, status,
                              execution_strategy, completed_subtasks_json, pending_subtasks_json,
                              modified_files_json, visited_files_json, verification_attempts,
                              verification_failures_json, executed_tools_summary_json, telemetry_json,
                              schema_version, created_at, updated_at
                       FROM task_checkpoints WHERE 1=1".to_string();

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(status) = status_filter {
            sql.push_str(" AND status = ?");
            params_vec.push(Box::new(status.as_str().to_string()));
        }
        sql.push_str(" ORDER BY updated_at DESC;");

        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(&params_refs[..], |row| {
            let schema_ver: i64 = row.get(16)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, i64>(12)?,
                row.get::<_, String>(13)?,
                row.get::<_, String>(14)?,
                row.get::<_, String>(15)?,
                schema_ver as u32,
                row.get::<_, i64>(17)? as u64,
                row.get::<_, i64>(18)? as u64,
            ))
        })?;

        let mut results = Vec::new();
        for r in rows {
            let tuple = r?;
            results.push(Self::parse_row_tuple(tuple)?);
        }

        Ok(results)
    }

    pub fn delete_checkpoint(&self, task_id: &str) -> Result<bool, TaskStoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| TaskStoreError::Lock(e.to_string()))?;

        let count = conn.execute(
            "DELETE FROM task_checkpoints WHERE task_id = ?1;",
            params![task_id],
        )?;
        Ok(count > 0)
    }

    pub fn query_recoverable_tasks(&self) -> Result<Vec<TaskCheckpoint>, TaskStoreError> {
        let all = self.list_checkpoints(None)?;
        Ok(all
            .into_iter()
            .filter(|cp| cp.status.is_recoverable_candidate())
            .collect())
    }

    #[allow(clippy::type_complexity)]
    fn parse_row_tuple(
        raw: (
            String,
            String,
            String,
            Option<String>,
            String,
            String,
            String,
            Option<String>,
            String,
            String,
            String,
            String,
            i64,
            String,
            String,
            String,
            u32,
            u64,
            u64,
        ),
    ) -> Result<TaskCheckpoint, TaskStoreError> {
        let (
            task_id,
            workspace_root,
            workspace_hash,
            git_commit,
            goal,
            phase,
            status_str,
            execution_strategy,
            completed_subtasks_json,
            pending_subtasks_json,
            modified_files_json,
            visited_files_json,
            verification_attempts,
            verification_failures_json,
            executed_tools_summary_json,
            telemetry_json,
            schema_version,
            created_at,
            updated_at,
        ) = raw;

        if schema_version > CURRENT_SCHEMA_VERSION {
            return Err(TaskStoreError::UnsupportedSchemaVersion {
                stored: schema_version,
                current: CURRENT_SCHEMA_VERSION,
            });
        }

        let status = TaskStatus::parse(&status_str)?;

        let completed_subtasks = serde_json::from_str(&completed_subtasks_json).map_err(|e| {
            TaskStoreError::CorruptedData {
                task_id: task_id.clone(),
                details: format!("completed_subtasks JSON invalid: {}", e),
            }
        })?;

        let pending_subtasks = serde_json::from_str(&pending_subtasks_json).map_err(|e| {
            TaskStoreError::CorruptedData {
                task_id: task_id.clone(),
                details: format!("pending_subtasks JSON invalid: {}", e),
            }
        })?;

        let modified_files = serde_json::from_str(&modified_files_json).map_err(|e| {
            TaskStoreError::CorruptedData {
                task_id: task_id.clone(),
                details: format!("modified_files JSON invalid: {}", e),
            }
        })?;

        let visited_files = serde_json::from_str(&visited_files_json).map_err(|e| {
            TaskStoreError::CorruptedData {
                task_id: task_id.clone(),
                details: format!("visited_files JSON invalid: {}", e),
            }
        })?;

        let verification_failures =
            serde_json::from_str(&verification_failures_json).map_err(|e| {
                TaskStoreError::CorruptedData {
                    task_id: task_id.clone(),
                    details: format!("verification_failures JSON invalid: {}", e),
                }
            })?;

        let executed_tools_summary =
            serde_json::from_str(&executed_tools_summary_json).map_err(|e| {
                TaskStoreError::CorruptedData {
                    task_id: task_id.clone(),
                    details: format!("executed_tools_summary JSON invalid: {}", e),
                }
            })?;

        let telemetry =
            serde_json::from_str(&telemetry_json).map_err(|e| TaskStoreError::CorruptedData {
                task_id: task_id.clone(),
                details: format!("telemetry JSON invalid: {}", e),
            })?;

        Ok(TaskCheckpoint {
            task_id,
            workspace_root,
            workspace_hash,
            git_commit,
            goal,
            phase,
            status,
            execution_strategy,
            completed_subtasks,
            pending_subtasks,
            modified_files,
            visited_files,
            verification_attempts: verification_attempts as usize,
            verification_failures,
            executed_tools_summary,
            telemetry,
            schema_version,
            created_at,
            updated_at,
        })
    }
}
