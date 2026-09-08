use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureCategory {
    SyntacticCompile,
    LogicTest,
    ContextOverflow,
    ToolEnvironment,
    GitConflict,
    ProviderModel,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryAction {
    RetryWorkerWithErrorPrompt,
    CompactContextAndRetry,
    EscalateModelReasoningTier,
    ReplanTaskGraph,
    SpawnMergeConflictResolver,
    RetryProviderFallback,
    EscalateToUser,
}

pub struct RecoveryEngine;

impl RecoveryEngine {
    pub fn classify_error(error_msg: &str) -> FailureCategory {
        let msg = error_msg.to_lowercase();
        if msg.contains("error[e")
            || msg.contains("cannot find")
            || msg.contains("mismatched types")
        {
            FailureCategory::SyntacticCompile
        } else if msg.contains("test failed") || msg.contains("assertion failed") {
            FailureCategory::LogicTest
        } else if msg.contains("context length")
            || msg.contains("token limit")
            || msg.contains("out of tokens")
        {
            FailureCategory::ContextOverflow
        } else if msg.contains("permission denied")
            || msg.contains("command not found")
            || msg.contains("file lock")
        {
            FailureCategory::ToolEnvironment
        } else if msg.contains("merge conflict") || msg.contains("conflict marker") {
            FailureCategory::GitConflict
        } else if msg.contains("rate limit")
            || msg.contains("500 internal")
            || msg.contains("bad gateway")
        {
            FailureCategory::ProviderModel
        } else {
            FailureCategory::Unknown
        }
    }

    pub fn determine_action(category: &FailureCategory, attempt: usize) -> RecoveryAction {
        match (category, attempt) {
            (FailureCategory::SyntacticCompile, 1..=2) => {
                RecoveryAction::RetryWorkerWithErrorPrompt
            }
            (FailureCategory::SyntacticCompile, _) => RecoveryAction::EscalateModelReasoningTier,
            (FailureCategory::LogicTest, 1..=2) => RecoveryAction::RetryWorkerWithErrorPrompt,
            (FailureCategory::LogicTest, _) => RecoveryAction::ReplanTaskGraph,
            (FailureCategory::ContextOverflow, _) => RecoveryAction::CompactContextAndRetry,
            (FailureCategory::ToolEnvironment, _) => RecoveryAction::EscalateToUser,
            (FailureCategory::GitConflict, _) => RecoveryAction::SpawnMergeConflictResolver,
            (FailureCategory::ProviderModel, _) => RecoveryAction::RetryProviderFallback,
            (FailureCategory::Unknown, 1) => RecoveryAction::RetryWorkerWithErrorPrompt,
            (FailureCategory::Unknown, _) => RecoveryAction::EscalateToUser,
        }
    }
}
