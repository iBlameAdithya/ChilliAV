//! chilli-memory: SQLite WAL state storage & Write-Ahead Event Journal.

pub mod journal;
pub mod persistent_memory;
pub mod recovery_policy;
pub mod storage;
pub mod task_store;

pub use persistent_memory::{
    ExtractedMemoryCandidate, MemoryCategory, MemoryEntry, MemoryError, MemoryQualityFilter,
    PersistentMemoryEngine,
};
pub use recovery_policy::{
    CurrentWorkspaceState, GitWorkspaceState, SafeResumeDecision, TaskRecoveryValidator,
    ValidationTelemetry, WorkspaceValidationResult,
};
pub use task_store::{
    CompactTaskTelemetry, TaskCheckpoint, TaskCheckpointStore, TaskSanitizer, TaskStatus,
    TaskStoreError, CURRENT_SCHEMA_VERSION,
};
