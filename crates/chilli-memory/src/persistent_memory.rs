use crate::storage::StoragePaths;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("Database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Mutex lock error: {0}")]
    Lock(String),
    #[error("Storage error: {0}")]
    Storage(#[from] crate::storage::StorageError),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MemoryCategory {
    Session,
    User,
    Feedback,
    Project,
    Architecture,
    Reference,
}

impl MemoryCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryCategory::Session => "session",
            MemoryCategory::User => "user",
            MemoryCategory::Feedback => "feedback",
            MemoryCategory::Project => "project",
            MemoryCategory::Architecture => "architecture",
            MemoryCategory::Reference => "reference",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "session" => Some(MemoryCategory::Session),
            "user" => Some(MemoryCategory::User),
            "feedback" => Some(MemoryCategory::Feedback),
            "project" => Some(MemoryCategory::Project),
            "architecture" | "decision" => Some(MemoryCategory::Architecture),
            "reference" | "fact" => Some(MemoryCategory::Reference),
            _ => None,
        }
    }

    pub fn base_weight(&self) -> f64 {
        match self {
            MemoryCategory::Architecture => 1.5,
            MemoryCategory::Project => 1.4,
            MemoryCategory::User => 1.3,
            MemoryCategory::Feedback => 1.2,
            MemoryCategory::Reference => 1.1,
            MemoryCategory::Session => 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryEntry {
    pub id: String,
    pub category: MemoryCategory,
    pub key: String,
    pub content: String,
    pub source: String,
    pub confidence: f64,
    pub created_at: u64,
    pub updated_at: u64,
    pub is_global: bool,
    pub is_stale: bool,
    pub superseded_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedMemoryCandidate {
    pub category: MemoryCategory,
    pub key: String,
    pub content: String,
    pub source: String,
    pub confidence: f64,
    pub is_global: bool,
}

pub struct MemoryQualityFilter;

impl MemoryQualityFilter {
    /// Quality gate: rejects credentials, secrets, raw transient logs, or empty noise.
    pub fn is_high_quality(category: MemoryCategory, key: &str, content: &str) -> bool {
        let key_lower = key.trim().to_lowercase();
        let content_lower = content.trim().to_lowercase();

        if key_lower.is_empty() || content_lower.is_empty() || content_lower.len() < 4 {
            return false;
        }

        // 1. Exclude security-sensitive patterns (credentials, .env files, private tokens)
        let excluded_patterns = [
            ".env",
            "aws_secret",
            "aws_access_key",
            "bearer ",
            "private_key",
            "api_key",
            "password",
            "secret_key",
        ];
        for pattern in &excluded_patterns {
            if key_lower.contains(pattern) || content_lower.contains(pattern) {
                return false;
            }
        }

        // 2. Filter out raw transient log outputs
        let transient_patterns = [
            "compiling ",
            "finished dev [unoptimized + debuginfo]",
            "running `target/debug",
            "exit code: 0",
            "exit status: 0",
            "stdout: ",
            "stderr: ",
        ];
        for pattern in &transient_patterns {
            if content_lower.contains(pattern) {
                return false;
            }
        }

        // 3. Reject generic non-informative entries
        let low_value_keys = ["ok", "done", "fixed", "test", "run", "log"];
        if low_value_keys.contains(&key_lower.as_str()) && content_lower.len() < 12 {
            return false;
        }

        // Allow all valid categories that pass filtering
        match category {
            MemoryCategory::Session
            | MemoryCategory::User
            | MemoryCategory::Feedback
            | MemoryCategory::Project
            | MemoryCategory::Architecture
            | MemoryCategory::Reference => true,
        }
    }
}

pub struct PersistentMemoryEngine {
    local_conn: Mutex<Connection>,
    global_conn: Mutex<Connection>,
}

impl PersistentMemoryEngine {
    pub fn new<P: AsRef<Path>>(workspace_root: P) -> Result<Self, MemoryError> {
        let paths = StoragePaths::resolve(workspace_root)?;
        Self::with_custom_paths(&paths.local_memory_db_path, &paths.global_memory_db_path)
    }

    pub fn with_custom_paths(local_db: &Path, global_db: &Path) -> Result<Self, MemoryError> {
        if let Some(parent) = local_db.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        if let Some(parent) = global_db.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let local_conn = Self::init_db(local_db)?;
        let global_conn = Self::init_db(global_db)?;

        Ok(Self {
            local_conn: Mutex::new(local_conn),
            global_conn: Mutex::new(global_conn),
        })
    }

    fn init_db(path: &Path) -> Result<Connection, MemoryError> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                category TEXT NOT NULL,
                key TEXT NOT NULL,
                content TEXT NOT NULL,
                source TEXT NOT NULL,
                confidence REAL NOT NULL DEFAULT 1.0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                is_stale INTEGER NOT NULL DEFAULT 0,
                superseded_by TEXT
            );",
            [],
        )?;

        // Migration safety for older databases without is_stale/superseded_by
        let _ = conn.execute(
            "ALTER TABLE memories ADD COLUMN is_stale INTEGER NOT NULL DEFAULT 0;",
            [],
        );
        let _ = conn.execute("ALTER TABLE memories ADD COLUMN superseded_by TEXT;", []);

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memories_category ON memories(category);",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memories_key ON memories(key);",
            [],
        )?;

        Ok(conn)
    }

    fn now_ts() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    pub fn store_local(
        &self,
        category: MemoryCategory,
        key: &str,
        content: &str,
        source: &str,
        confidence: f64,
    ) -> Result<MemoryEntry, MemoryError> {
        if !MemoryQualityFilter::is_high_quality(category, key, content) {
            return Err(MemoryError::Lock(format!(
                "Memory entry ('{}', '{}') failed quality filter",
                key, content
            )));
        }
        let conn = self
            .local_conn
            .lock()
            .map_err(|e| MemoryError::Lock(e.to_string()))?;
        Self::upsert_memory(&conn, category, key, content, source, confidence, false)
    }

    pub fn store_global(
        &self,
        category: MemoryCategory,
        key: &str,
        content: &str,
        source: &str,
        confidence: f64,
    ) -> Result<MemoryEntry, MemoryError> {
        if !MemoryQualityFilter::is_high_quality(category, key, content) {
            return Err(MemoryError::Lock(format!(
                "Memory entry ('{}', '{}') failed quality filter",
                key, content
            )));
        }
        let conn = self
            .global_conn
            .lock()
            .map_err(|e| MemoryError::Lock(e.to_string()))?;
        Self::upsert_memory(&conn, category, key, content, source, confidence, true)
    }

    pub fn mark_stale_local(
        &self,
        id: &str,
        superseded_by: Option<&str>,
    ) -> Result<bool, MemoryError> {
        let conn = self
            .local_conn
            .lock()
            .map_err(|e| MemoryError::Lock(e.to_string()))?;
        let count = conn.execute(
            "UPDATE memories SET is_stale = 1, superseded_by = ?1 WHERE id = ?2;",
            params![superseded_by, id],
        )?;
        Ok(count > 0)
    }

    pub fn mark_stale_global(
        &self,
        id: &str,
        superseded_by: Option<&str>,
    ) -> Result<bool, MemoryError> {
        let conn = self
            .global_conn
            .lock()
            .map_err(|e| MemoryError::Lock(e.to_string()))?;
        let count = conn.execute(
            "UPDATE memories SET is_stale = 1, superseded_by = ?1 WHERE id = ?2;",
            params![superseded_by, id],
        )?;
        Ok(count > 0)
    }

    fn upsert_memory(
        conn: &Connection,
        category: MemoryCategory,
        key: &str,
        content: &str,
        source: &str,
        confidence: f64,
        is_global: bool,
    ) -> Result<MemoryEntry, MemoryError> {
        let now = Self::now_ts();
        let cat_str = category.as_str();

        let mut stmt = conn.prepare(
            "SELECT id, created_at FROM memories WHERE category = ?1 AND key = ?2 LIMIT 1;",
        )?;
        let existing = stmt
            .query_row(params![cat_str, key], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
            })
            .ok();

        let (id, created_at) = if let Some((existing_id, c_at)) = existing {
            (existing_id, c_at)
        } else {
            (format!("{}:{}", cat_str, key), now)
        };

        conn.execute(
            "INSERT INTO memories (id, category, key, content, source, confidence, created_at, updated_at, is_stale, superseded_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, NULL)
             ON CONFLICT(id) DO UPDATE SET
                content = excluded.content,
                source = excluded.source,
                confidence = excluded.confidence,
                updated_at = excluded.updated_at,
                is_stale = 0,
                superseded_by = NULL;",
            params![
                id,
                cat_str,
                key,
                content,
                source,
                confidence,
                created_at as i64,
                now as i64,
            ],
        )?;

        Ok(MemoryEntry {
            id,
            category,
            key: key.to_string(),
            content: content.to_string(),
            source: source.to_string(),
            confidence,
            created_at,
            updated_at: now,
            is_global,
            is_stale: false,
            superseded_by: None,
        })
    }

    pub fn recall_local(
        &self,
        category: Option<MemoryCategory>,
        query: Option<&str>,
    ) -> Result<Vec<MemoryEntry>, MemoryError> {
        let conn = self
            .local_conn
            .lock()
            .map_err(|e| MemoryError::Lock(e.to_string()))?;
        Self::query_memories(&conn, category, query, false, false)
    }

    pub fn recall_global(
        &self,
        category: Option<MemoryCategory>,
        query: Option<&str>,
    ) -> Result<Vec<MemoryEntry>, MemoryError> {
        let conn = self
            .global_conn
            .lock()
            .map_err(|e| MemoryError::Lock(e.to_string()))?;
        Self::query_memories(&conn, category, query, true, false)
    }

    pub fn recall_combined(
        &self,
        category: Option<MemoryCategory>,
        query: Option<&str>,
    ) -> Result<Vec<MemoryEntry>, MemoryError> {
        let mut local = self.recall_local(category, query)?;
        let global = self.recall_global(category, query)?;

        let local_keys: std::collections::HashSet<(MemoryCategory, String)> =
            local.iter().map(|m| (m.category, m.key.clone())).collect();

        for g_mem in global {
            if !local_keys.contains(&(g_mem.category, g_mem.key.clone())) {
                local.push(g_mem);
            }
        }

        local.sort_by_key(|b| std::cmp::Reverse(b.updated_at));
        Ok(local)
    }

    pub fn recall_ranked(
        &self,
        active_prompt: &str,
        active_files: &[String],
        active_symbols: &[String],
        limit: usize,
    ) -> Result<Vec<MemoryEntry>, MemoryError> {
        let candidates = self.recall_combined(None, None)?;
        let mut scored: Vec<(f64, MemoryEntry)> = candidates
            .into_iter()
            .filter(|m| !m.is_stale)
            .map(|m| {
                let score = Self::calculate_relevance_score(
                    &m,
                    active_prompt,
                    active_files,
                    active_symbols,
                );
                (score, m)
            })
            .filter(|(score, _)| *score > 0.1)
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored.into_iter().map(|(_, m)| m).take(limit).collect())
    }

    fn calculate_relevance_score(
        entry: &MemoryEntry,
        active_prompt: &str,
        active_files: &[String],
        active_symbols: &[String],
    ) -> f64 {
        let mut score = entry.confidence * entry.category.base_weight();

        let prompt_words: Vec<&str> = active_prompt
            .split_whitespace()
            .filter(|w| w.len() > 3)
            .collect();

        for word in &prompt_words {
            let w_lower = word.to_lowercase();
            if entry.key.to_lowercase().contains(&w_lower) {
                score += 1.5;
            }
            if entry.content.to_lowercase().contains(&w_lower) {
                score += 0.8;
            }
        }

        for file_path in active_files {
            let f_lower = file_path.to_lowercase();
            if entry.content.to_lowercase().contains(&f_lower)
                || entry.key.to_lowercase().contains(&f_lower)
            {
                score += 2.0;
            }
        }

        for symbol in active_symbols {
            let s_lower = symbol.to_lowercase();
            if entry.content.to_lowercase().contains(&s_lower)
                || entry.key.to_lowercase().contains(&s_lower)
            {
                score += 2.5;
            }
        }

        score
    }

    fn query_memories(
        conn: &Connection,
        category: Option<MemoryCategory>,
        query: Option<&str>,
        is_global: bool,
        include_stale: bool,
    ) -> Result<Vec<MemoryEntry>, MemoryError> {
        let mut sql = "SELECT id, category, key, content, source, confidence, created_at, updated_at, is_stale, superseded_by FROM memories WHERE 1=1".to_string();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if !include_stale {
            sql.push_str(" AND is_stale = 0");
        }

        if let Some(cat) = category {
            sql.push_str(" AND category = ?");
            params_vec.push(Box::new(cat.as_str().to_string()));
        }

        if let Some(q) = query {
            if !q.trim().is_empty() {
                sql.push_str(" AND (key LIKE ? OR content LIKE ?)");
                let pattern = format!("%{}%", q.trim());
                params_vec.push(Box::new(pattern.clone()));
                params_vec.push(Box::new(pattern));
            }
        }

        sql.push_str(" ORDER BY updated_at DESC;");

        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(&params_refs[..], |row| {
            let cat_str: String = row.get(1)?;
            let category = MemoryCategory::parse(&cat_str).unwrap_or(MemoryCategory::Project);
            let is_stale_val: i64 = row.get(8)?;
            Ok(MemoryEntry {
                id: row.get(0)?,
                category,
                key: row.get(2)?,
                content: row.get(3)?,
                source: row.get(4)?,
                confidence: row.get(5)?,
                created_at: row.get::<_, i64>(6)? as u64,
                updated_at: row.get::<_, i64>(7)? as u64,
                is_global,
                is_stale: is_stale_val == 1,
                superseded_by: row.get(9)?,
            })
        })?;

        let mut entries = Vec::new();
        for r in rows {
            entries.push(r?);
        }

        Ok(entries)
    }

    pub fn delete_local(&self, id: &str) -> Result<bool, MemoryError> {
        let conn = self
            .local_conn
            .lock()
            .map_err(|e| MemoryError::Lock(e.to_string()))?;
        let count = conn.execute("DELETE FROM memories WHERE id = ?1;", params![id])?;
        Ok(count > 0)
    }

    pub fn delete_global(&self, id: &str) -> Result<bool, MemoryError> {
        let conn = self
            .global_conn
            .lock()
            .map_err(|e| MemoryError::Lock(e.to_string()))?;
        let count = conn.execute("DELETE FROM memories WHERE id = ?1;", params![id])?;
        Ok(count > 0)
    }

    /// Extract durable memory candidates from user prompts and assistant outputs.
    pub fn extract_memories_from_turn(
        user_prompt: &str,
        assistant_response: &str,
    ) -> Vec<ExtractedMemoryCandidate> {
        let mut candidates = Vec::new();
        let prompt_lower = user_prompt.to_lowercase();
        let resp_lower = assistant_response.to_lowercase();

        // 1. User preferences & conventions
        if prompt_lower.contains("always ")
            || prompt_lower.contains("prefer ")
            || prompt_lower.contains("never ")
        {
            candidates.push(ExtractedMemoryCandidate {
                category: MemoryCategory::User,
                key: "user_preference".to_string(),
                content: user_prompt.trim().to_string(),
                source: "user_prompt_extraction".to_string(),
                confidence: 0.9,
                is_global: true,
            });
        }

        // 2. Architecture & decision conventions
        if resp_lower.contains("architecture decision")
            || resp_lower.contains("decided to")
            || resp_lower.contains("conventions:")
        {
            candidates.push(ExtractedMemoryCandidate {
                category: MemoryCategory::Architecture,
                key: "arch_convention".to_string(),
                content: assistant_response
                    .lines()
                    .next()
                    .unwrap_or("Architecture decision")
                    .trim()
                    .to_string(),
                source: "assistant_turn_extraction".to_string(),
                confidence: 0.85,
                is_global: false,
            });
        }

        candidates
    }
}
