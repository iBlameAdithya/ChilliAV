use chilli_model::router::Requirement;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
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
    Completed {
        task_id: String,
        agent_id: String,
        success: bool,
    },
    Failed {
        task_id: String,
        agent_id: String,
        reason: String,
    },
}

#[derive(Error, Debug)]
pub enum AgentError {
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Configuration error: {0}")]
    ConfigError(String),
    #[error("Tool error: {0}")]
    ToolError(String),
}

// --- Legacy Verification Types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VerificationType {
    FileContains { path: String, expected: String },
    FileExists { path: String },
    CommandExitZero { command: String, args: Vec<String> },
    GitDiffNotEmpty,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VerificationSpec {
    pub checks: Vec<VerificationType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationFailure {
    pub check: VerificationType,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyVerificationResult {
    pub passed: bool,
    pub failures: Vec<VerificationFailure>,
}

pub struct VerificationEngine {
    workspace_root: PathBuf,
}

impl VerificationEngine {
    pub fn new(workspace_root: &Path) -> Self {
        Self {
            workspace_root: workspace_root.to_path_buf(),
        }
    }

    pub fn verify(&self, spec: &VerificationSpec) -> LegacyVerificationResult {
        let mut failures = Vec::new();

        for check in &spec.checks {
            match check {
                VerificationType::FileContains { path, expected } => {
                    let target_path = self.workspace_root.join(path);
                    match std::fs::read_to_string(&target_path) {
                        Ok(content) => {
                            if !content.contains(expected) {
                                failures.push(VerificationFailure {
                                    check: check.clone(),
                                    reason: format!(
                                        "File '{}' does not contain expected substring '{}'",
                                        path, expected
                                    ),
                                });
                            }
                        }
                        Err(e) => {
                            failures.push(VerificationFailure {
                                check: check.clone(),
                                reason: format!("Failed to read file '{}': {}", path, e),
                            });
                        }
                    }
                }
                VerificationType::FileExists { path } => {
                    let target_path = self.workspace_root.join(path);
                    if !target_path.exists() {
                        failures.push(VerificationFailure {
                            check: check.clone(),
                            reason: format!("Expected file '{}' does not exist", path),
                        });
                    }
                }
                VerificationType::CommandExitZero { command, args } => {
                    let mut cmd = std::process::Command::new(command);
                    cmd.args(args);
                    cmd.current_dir(&self.workspace_root);
                    match cmd.output() {
                        Ok(output) => {
                            if !output.status.success() {
                                let stderr = String::from_utf8_lossy(&output.stderr);
                                failures.push(VerificationFailure {
                                    check: check.clone(),
                                    reason: format!(
                                        "Command '{} {:?}' exited with non-zero status code: {}. stderr: {}",
                                        command, args, output.status, stderr
                                    ),
                                });
                            }
                        }
                        Err(e) => {
                            failures.push(VerificationFailure {
                                check: check.clone(),
                                reason: format!(
                                    "Failed to execute command '{} {:?}': {}",
                                    command, args, e
                                ),
                            });
                        }
                    }
                }
                VerificationType::GitDiffNotEmpty => {
                    let mut cmd = std::process::Command::new("git");
                    cmd.args(["diff", "--quiet"]);
                    cmd.current_dir(&self.workspace_root);
                    match cmd.output() {
                        Ok(output) => {
                            if output.status.code() == Some(0) {
                                failures.push(VerificationFailure {
                                    check: check.clone(),
                                    reason: "Working directory has no uncommitted git diffs"
                                        .to_string(),
                                });
                            }
                        }
                        Err(e) => {
                            failures.push(VerificationFailure {
                                check: check.clone(),
                                reason: format!("Failed to check git diff: {}", e),
                            });
                        }
                    }
                }
            }
        }

        LegacyVerificationResult {
            passed: failures.is_empty(),
            failures,
        }
    }
}

// --- Orchestrator Pipeline & Graph Types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPlan {
    pub plan_id: String,
    pub goal: String,
    pub tasks: Vec<PlannerTaskSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannerTaskSpec {
    pub id: String,
    pub description: String,
    pub role: AgentRole,
    pub depends_on: Vec<String>,
    pub expected_artifacts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Ready,
    Running { agent_id: String },
    Verifying { agent_id: String },
    Verified,
    Completed,
    Failed { reason: String },
    Skipped,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskNode {
    pub id: String,
    pub spec: PlannerTaskSpec,
    pub status: TaskStatus,
    pub output: Option<AgentOutput>,
}

#[derive(Error, Debug)]
pub enum OrchestratorError {
    #[error("Cyclic dependency detected in task plan: {0}")]
    CyclicDependency(String),
    #[error("Task execution failed: {0}")]
    TaskExecutionFailed(String),
    #[error("Verification failed: {0}")]
    VerificationFailed(String),
    #[error("Invalid plan structure: {0}")]
    InvalidPlan(String),
}
