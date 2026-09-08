use chilli_model::router::Requirement;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentRole {
    Planner,
    Worker,
    Verifier,
    Architect,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub agent_id: String,
    pub role: AgentRole,
    pub system_instruction_override: Option<String>,
    pub allowed_tools: Vec<String>,
    pub max_turns: usize,
    pub model_requirements: Vec<Requirement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInput {
    pub task_id: String,
    pub prompt: String,
    pub context_files: Vec<PathBuf>,
    pub structured_data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentOutput {
    pub task_id: String,
    pub agent_id: String,
    pub success: bool,
    pub summary: String,
    pub artifacts: Vec<PathBuf>,
    pub structured_result: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentEvent {
    Started {
        task_id: String,
        agent_id: String,
        role: AgentRole,
    },
    ToolCalled {
        task_id: String,
        agent_id: String,
        tool_name: String,
    },
    TurnCompleted {
        task_id: String,
        agent_id: String,
        turn: usize,
    },
    Failed {
        task_id: String,
        agent_id: String,
        error: String,
    },
    Completed {
        task_id: String,
        agent_id: String,
        success: bool,
    },
}

#[derive(Error, Debug)]
pub enum AgentError {
    #[error("Agent execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Tool error: {0}")]
    ToolError(String),
    #[error("Max turns exceeded: {0}")]
    MaxTurnsExceeded(usize),
}
