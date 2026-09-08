use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum JournalError {
    #[error("Database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Mutex lock error: {0}")]
    Lock(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventRecord {
    pub seq: u64,
    pub session_id: String,
    pub event_type: String,
    pub payload_json: String,
    pub timestamp_ms: u64,
}

pub struct EventJournal {
    conn: Mutex<Connection>,
}

impl EventJournal {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, JournalError> {
        let conn = Connection::open(path)?;

        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS events (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                timestamp_ms INTEGER NOT NULL
            );",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_events_session ON events (session_id, seq);",
            [],
        )?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn append(&self, record: &EventRecord) -> Result<u64, JournalError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| JournalError::Lock(e.to_string()))?;

        conn.execute(
            "INSERT INTO events (session_id, event_type, payload_json, timestamp_ms) VALUES (?1, ?2, ?3, ?4);",
            params![
                record.session_id,
                record.event_type,
                record.payload_json,
                record.timestamp_ms as i64,
            ],
        )?;

        let seq = conn.last_insert_rowid() as u64;
        Ok(seq)
    }

    pub fn events_after(
        &self,
        session_id: &str,
        after_seq: u64,
    ) -> Result<Vec<EventRecord>, JournalError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| JournalError::Lock(e.to_string()))?;

        let mut stmt = conn.prepare(
            "SELECT seq, session_id, event_type, payload_json, timestamp_ms FROM events WHERE session_id = ?1 AND seq > ?2 ORDER BY seq ASC;",
        )?;

        let rows = stmt.query_map(params![session_id, after_seq as i64], |row| {
            let timestamp_raw: i64 = row.get(4)?;
            Ok(EventRecord {
                seq: row.get::<_, i64>(0)? as u64,
                session_id: row.get(1)?,
                event_type: row.get(2)?,
                payload_json: row.get(3)?,
                timestamp_ms: timestamp_raw as u64,
            })
        })?;

        let mut events = Vec::new();
        for r in rows {
            events.push(r?);
        }

        Ok(events)
    }

    pub fn latest_sequence_id(&self, session_id: &str) -> Result<u64, JournalError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| JournalError::Lock(e.to_string()))?;

        let mut stmt =
            conn.prepare("SELECT COALESCE(MAX(seq), 0) FROM events WHERE session_id = ?1;")?;

        let max_seq: i64 = stmt.query_row(params![session_id], |row| row.get(0))?;
        Ok(max_seq as u64)
    }
}
