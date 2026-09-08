use chilli_context::types::TokenTelemetry;
use chilli_memory::{CompactTaskTelemetry, TaskCheckpoint, TaskStatus, CURRENT_SCHEMA_VERSION};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionState {
    Idle,
    AssemblingContext,
    RequestingModel,
    StreamingResponse,
    ParsingToolCalls,
    EvaluatingPolicy,
    ExecutingTools,
    RecordingObservations,
    CheckingCompletion,
    Finished,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AutonomyMode {
    #[default]
    Auto,
    Plan,
    Manual,
}

impl AutonomyMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            AutonomyMode::Auto => "auto",
            AutonomyMode::Plan => "plan",
            AutonomyMode::Manual => "manual",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "plan" => AutonomyMode::Plan,
            "manual" | "step" => AutonomyMode::Manual,
            _ => AutonomyMode::Auto,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineEvent {
    TurnStarted {
        session_id: String,
        prompt: String,
    },
    PhaseChanged {
        phase: TaskPhase,
    },
    ExecutionStateChanged {
        state: ExecutionState,
    },
    ModelStreamChunk {
        delta: String,
    },
    ToolCallStarted {
        tool_name: String,
        args_json: String,
    },
    ToolExecutionCompleted {
        tool_name: String,
        result_summary: String,
        duration_ms: u64,
    },
    ToolCallDenied {
        tool_name: String,
        reason: String,
    },
    PlanAwaitingApproval {
        plan_summary: String,
    },
    MemoryRecalled {
        count: usize,
        categories: Vec<String>,
    },
    TurnCompleted {
        token_usage: TokenTelemetry,
        duration_ms: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanApprovalResponse {
    Approve,
    Edit(String),
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlEvent {
    PlanApproval(PlanApprovalResponse),
    UserInterruption,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TaskPhase {
    #[default]
    Understand,
    Inspect,
    Plan,
    Execute,
    Verify,
    Recover,
    Finished,
    Failed,
}

impl TaskPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskPhase::Understand => "understand",
            TaskPhase::Inspect => "inspect",
            TaskPhase::Plan => "plan",
            TaskPhase::Execute => "execute",
            TaskPhase::Verify => "verify",
            TaskPhase::Recover => "recover",
            TaskPhase::Finished => "finished",
            TaskPhase::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "understand" => TaskPhase::Understand,
            "inspect" => TaskPhase::Inspect,
            "plan" => TaskPhase::Plan,
            "execute" | "implementation" => TaskPhase::Execute,
            "verify" => TaskPhase::Verify,
            "recover" => TaskPhase::Recover,
            "finished" | "completed" => TaskPhase::Finished,
            "failed" => TaskPhase::Failed,
            _ => TaskPhase::Understand,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScopeType {
    SingleFile,
    MultiFile,
    WorkspaceWide,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationStatus {
    Unverified,
    Passed,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CheckpointState {
    pub completed_subtasks: Vec<String>,
    pub pending_subtasks: Vec<String>,
    pub modified_files: Vec<String>,
    pub verification_matrix: HashMap<String, VerificationStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRequirements {
    pub requires_workspace: bool,
    pub artifact_scope: ScopeType,
    pub cross_file_dependency: bool,
    pub requires_verification: bool,
    pub estimated_iteration_depth: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionStrategy {
    DirectResponse,
    SingleTurnEdit,
    MultiTurnStateMachine {
        max_turns: usize,
    },
    CheckpointedLoop {
        base_budget: usize,
        elastic_extension: usize,
    },
}

impl ExecutionStrategy {
    pub fn max_turns(&self) -> usize {
        match self {
            ExecutionStrategy::DirectResponse => 1,
            ExecutionStrategy::SingleTurnEdit => 6,
            ExecutionStrategy::MultiTurnStateMachine { max_turns } => *max_turns,
            ExecutionStrategy::CheckpointedLoop {
                base_budget,
                elastic_extension,
            } => base_budget + elastic_extension,
        }
    }

    pub fn as_str(&self) -> String {
        match self {
            ExecutionStrategy::DirectResponse => "direct".to_string(),
            ExecutionStrategy::SingleTurnEdit => "single_turn".to_string(),
            ExecutionStrategy::MultiTurnStateMachine { max_turns } => {
                format!("multi_turn:{}", max_turns)
            }
            ExecutionStrategy::CheckpointedLoop {
                base_budget,
                elastic_extension,
            } => {
                format!("checkpointed:{}:{}", base_budget, elastic_extension)
            }
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let s_lower = s.trim().to_lowercase();
        if s_lower == "direct" || s_lower == "direct_response" {
            Some(ExecutionStrategy::DirectResponse)
        } else if s_lower == "single_turn" || s_lower == "single_turn_edit" {
            Some(ExecutionStrategy::SingleTurnEdit)
        } else if s_lower.starts_with("multi_turn:") {
            let parts: Vec<&str> = s_lower.split(':').collect();
            let max_turns = parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(6);
            Some(ExecutionStrategy::MultiTurnStateMachine { max_turns })
        } else if s_lower.starts_with("checkpointed:") {
            let parts: Vec<&str> = s_lower.split(':').collect();
            let base_budget = parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(5);
            let elastic_extension = parts.get(2).and_then(|p| p.parse().ok()).unwrap_or(10);
            Some(ExecutionStrategy::CheckpointedLoop {
                base_budget,
                elastic_extension,
            })
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveTaskState {
    pub task_id: Option<String>,
    pub goal: String,
    pub phase: TaskPhase,
    pub modified_files: Vec<String>,
    pub visited_files: Vec<String>,
    pub verification_attempts: usize,
    pub consecutive_failures: usize,
    pub executed_tools_summary: Vec<String>,
    pub zero_tool_guard_triggered: bool,
    pub verification_failures: Vec<String>,
    pub checkpoint: CheckpointState,
    pub requirements: Option<TaskRequirements>,
    pub strategy: Option<ExecutionStrategy>,
    pub telemetry: TokenTelemetry,
}

impl ActiveTaskState {
    pub fn new(goal: impl Into<String>) -> Self {
        let goal_str = goal.into();
        let requirements = classify_task_requirements(&goal_str, &[]);
        let strategy = select_execution_strategy(&requirements);
        Self {
            task_id: None,
            goal: goal_str,
            phase: TaskPhase::Understand,
            modified_files: Vec::new(),
            visited_files: Vec::new(),
            verification_attempts: 0,
            consecutive_failures: 0,
            executed_tools_summary: Vec::new(),
            zero_tool_guard_triggered: false,
            verification_failures: Vec::new(),
            checkpoint: CheckpointState::default(),
            requirements: Some(requirements),
            strategy: Some(strategy),
            telemetry: TokenTelemetry::default(),
        }
    }

    pub fn try_from_checkpoint(cp: &TaskCheckpoint) -> Result<Self, String> {
        if cp.schema_version != CURRENT_SCHEMA_VERSION {
            return Err(format!(
                "Unsupported schema version: {}, expected {}",
                cp.schema_version, CURRENT_SCHEMA_VERSION
            ));
        }
        if cp.task_id.trim().is_empty() {
            return Err("Checkpoint task_id cannot be empty".to_string());
        }
        if cp.goal.trim().is_empty() {
            return Err("Checkpoint goal cannot be empty".to_string());
        }
        if let Some(ref s) = cp.execution_strategy {
            if ExecutionStrategy::parse(s).is_none() {
                return Err(format!("Unrecognized execution strategy format: '{s}'"));
            }
        }

        let phase = TaskPhase::parse(&cp.phase);
        let strategy = cp
            .execution_strategy
            .as_deref()
            .and_then(ExecutionStrategy::parse);
        let requirements = classify_task_requirements(&cp.goal, &cp.visited_files);

        let mut telemetry = TokenTelemetry::default();
        telemetry.input_tokens = cp.telemetry.input_tokens;
        telemetry.output_tokens = cp.telemetry.output_tokens;
        telemetry.cached_tokens = cp.telemetry.cached_tokens;
        telemetry.total_task_tokens = cp.telemetry.total_tokens;

        let checkpoint_state = CheckpointState {
            completed_subtasks: cp.completed_subtasks.clone(),
            pending_subtasks: cp.pending_subtasks.clone(),
            modified_files: cp.modified_files.clone(),
            verification_matrix: HashMap::new(),
        };

        Ok(Self {
            task_id: Some(cp.task_id.clone()),
            goal: cp.goal.clone(),
            phase,
            modified_files: cp.modified_files.clone(),
            visited_files: cp.visited_files.clone(),
            verification_attempts: cp.verification_attempts,
            consecutive_failures: 0,
            executed_tools_summary: cp.executed_tools_summary.clone(),
            zero_tool_guard_triggered: false,
            verification_failures: cp.verification_failures.clone(),
            checkpoint: checkpoint_state,
            requirements: Some(requirements),
            strategy,
            telemetry,
        })
    }

    pub fn from_checkpoint(cp: &TaskCheckpoint) -> Self {
        Self::try_from_checkpoint(cp).unwrap_or_else(|_| {
            let phase = TaskPhase::parse(&cp.phase);
            let strategy = cp
                .execution_strategy
                .as_deref()
                .and_then(ExecutionStrategy::parse);
            let requirements = classify_task_requirements(&cp.goal, &cp.visited_files);

            let mut telemetry = TokenTelemetry::default();
            telemetry.input_tokens = cp.telemetry.input_tokens;
            telemetry.output_tokens = cp.telemetry.output_tokens;
            telemetry.cached_tokens = cp.telemetry.cached_tokens;
            telemetry.total_task_tokens = cp.telemetry.total_tokens;

            let checkpoint_state = CheckpointState {
                completed_subtasks: cp.completed_subtasks.clone(),
                pending_subtasks: cp.pending_subtasks.clone(),
                modified_files: cp.modified_files.clone(),
                verification_matrix: HashMap::new(),
            };

            Self {
                task_id: if cp.task_id.trim().is_empty() {
                    None
                } else {
                    Some(cp.task_id.clone())
                },
                goal: cp.goal.clone(),
                phase,
                modified_files: cp.modified_files.clone(),
                visited_files: cp.visited_files.clone(),
                verification_attempts: cp.verification_attempts,
                consecutive_failures: 0,
                executed_tools_summary: cp.executed_tools_summary.clone(),
                zero_tool_guard_triggered: false,
                verification_failures: cp.verification_failures.clone(),
                checkpoint: checkpoint_state,
                requirements: Some(requirements),
                strategy,
                telemetry,
            }
        })
    }

    pub fn to_checkpoint(
        &self,
        task_id: &str,
        workspace_root: &str,
        workspace_hash: &str,
        git_commit: Option<String>,
        status: TaskStatus,
    ) -> TaskCheckpoint {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        TaskCheckpoint {
            task_id: task_id.to_string(),
            workspace_root: workspace_root.to_string(),
            workspace_hash: workspace_hash.to_string(),
            git_commit,
            goal: self.goal.clone(),
            phase: self.phase.as_str().to_string(),
            status,
            execution_strategy: self.strategy.as_ref().map(|s| s.as_str()),
            completed_subtasks: self.checkpoint.completed_subtasks.clone(),
            pending_subtasks: self.checkpoint.pending_subtasks.clone(),
            modified_files: self.modified_files.clone(),
            visited_files: self.visited_files.clone(),
            verification_attempts: self.verification_attempts,
            verification_failures: self.verification_failures.clone(),
            executed_tools_summary: self.executed_tools_summary.clone(),
            telemetry: CompactTaskTelemetry {
                input_tokens: self.telemetry.input_tokens,
                output_tokens: self.telemetry.output_tokens,
                cached_tokens: self.telemetry.cached_tokens,
                total_tokens: if self.telemetry.total_task_tokens > 0 {
                    self.telemetry.total_task_tokens
                } else {
                    self.telemetry.input_tokens + self.telemetry.output_tokens
                },
                total_turns: 0,
                tool_calls_count: self.executed_tools_summary.len(),
            },
            schema_version: CURRENT_SCHEMA_VERSION,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn record_file_modification(&mut self, path: &str) {
        if !self.modified_files.iter().any(|f| f == path) {
            self.modified_files.push(path.to_string());
        }
        if !self.checkpoint.modified_files.iter().any(|f| f == path) {
            self.checkpoint.modified_files.push(path.to_string());
        }
        self.record_file_visit(path);
    }

    pub fn record_file_visit(&mut self, path: &str) {
        if !self.visited_files.iter().any(|f| f == path) {
            self.visited_files.push(path.to_string());
        }
    }

    pub fn render_working_memory_summary(&self) -> String {
        let modified = if self.modified_files.is_empty() {
            "none".to_string()
        } else {
            self.modified_files.join(", ")
        };

        let visited = if self.visited_files.is_empty() {
            "none".to_string()
        } else {
            let keys: Vec<_> = self.visited_files.iter().take(5).cloned().collect();
            keys.join(", ")
        };

        let recent_tools = if self.executed_tools_summary.is_empty() {
            "none".to_string()
        } else {
            let start = self.executed_tools_summary.len().saturating_sub(3);
            self.executed_tools_summary[start..].join("; ")
        };

        format!(
            "[WORKING MEMORY]\n\
             Task Goal: {}\n\
             Execution Phase: {:?}\n\
             Modified Files: [{}]\n\
             Visited Files: [{}]\n\
             Recent Tool Actions: {}\n\
             Verification Attempts: {}",
            self.goal, self.phase, modified, visited, recent_tools, self.verification_attempts
        )
    }

    pub fn is_workspace_task(&self) -> bool {
        self.requirements
            .as_ref()
            .map(|r| r.requires_workspace)
            .unwrap_or_else(|| is_workspace_task(&self.goal))
    }
}

pub fn classify_task_requirements(goal: &str, known_files: &[String]) -> TaskRequirements {
    let goal_trim = goal.trim();
    let goal_lower = goal_trim.to_lowercase();
    let is_ws = is_workspace_task(goal_trim);

    if !is_ws {
        return TaskRequirements {
            requires_workspace: false,
            artifact_scope: ScopeType::Unknown,
            cross_file_dependency: false,
            requires_verification: false,
            estimated_iteration_depth: 1,
        };
    }

    // Count file mentions
    let file_extensions = [
        ".rs", ".py", ".toml", ".txt", ".js", ".ts", ".json", ".md", ".sh", ".ps1",
    ];
    let mentioned_file_count = known_files.len()
        + file_extensions
            .iter()
            .filter(|ext| goal_lower.contains(*ext))
            .count();

    let has_repo_wide_indicator = goal_lower.contains("repo")
        || goal_lower.contains("workspace")
        || goal_lower.contains("all files")
        || goal_lower.contains("entire project")
        || goal_lower.contains("across the codebase")
        || goal_lower.contains("suite");

    let artifact_scope = if has_repo_wide_indicator || mentioned_file_count > 3 {
        ScopeType::WorkspaceWide
    } else if mentioned_file_count >= 2
        || goal_lower.contains("refactor")
        || goal_lower.contains("dependency")
        || goal_lower.contains("module")
    {
        ScopeType::MultiFile
    } else {
        ScopeType::SingleFile
    };

    let cross_file_dependency = artifact_scope == ScopeType::MultiFile
        || artifact_scope == ScopeType::WorkspaceWide
        || goal_lower.contains("callers")
        || goal_lower.contains("trait")
        || goal_lower.contains("struct")
        || goal_lower.contains("import")
        || goal_lower.contains("dependency");

    let requires_verification = goal_lower.contains("test")
        || goal_lower.contains("verify")
        || goal_lower.contains("check")
        || goal_lower.contains("assert")
        || goal_lower.contains("build")
        || goal_lower.contains("benchmark")
        || goal_lower.contains("validate");

    let estimated_iteration_depth = match artifact_scope {
        ScopeType::SingleFile if !requires_verification => 2,
        ScopeType::SingleFile => 4,
        ScopeType::MultiFile => 8,
        ScopeType::WorkspaceWide => 14,
        ScopeType::Unknown => 5,
    };

    TaskRequirements {
        requires_workspace: true,
        artifact_scope,
        cross_file_dependency,
        requires_verification,
        estimated_iteration_depth,
    }
}

pub fn select_execution_strategy(req: &TaskRequirements) -> ExecutionStrategy {
    let adaptive_policy = crate::ABLATION_ADAPTIVE_POLICY.load(std::sync::atomic::Ordering::SeqCst);
    if !adaptive_policy {
        return ExecutionStrategy::CheckpointedLoop {
            base_budget: 15,
            elastic_extension: 0,
        };
    }
    if !req.requires_workspace {
        ExecutionStrategy::DirectResponse
    } else if req.artifact_scope == ScopeType::SingleFile
        && !req.cross_file_dependency
        && !req.requires_verification
    {
        ExecutionStrategy::SingleTurnEdit
    } else if req.artifact_scope == ScopeType::WorkspaceWide || req.estimated_iteration_depth >= 10
    {
        ExecutionStrategy::CheckpointedLoop {
            base_budget: 10,
            elastic_extension: 8,
        }
    } else {
        ExecutionStrategy::MultiTurnStateMachine { max_turns: 10 }
    }
}

pub fn is_workspace_task(task: &str) -> bool {
    let task_trim = task.trim();
    let task_lower = task_trim.to_lowercase();

    // Conversational or pure Q&A queries
    let clean_alpha: String = task_lower.chars().filter(|c| c.is_alphabetic() || c.is_whitespace()).collect();
    let clean_alpha_trim = clean_alpha.trim();
    if matches!(
        clean_alpha_trim,
        "heya" | "hello" | "hi" | "hey" | "yo" | "sup" | "howdy" | "greetings"
            | "good morning" | "good afternoon" | "good evening" | "how are you" | "whats up"
    ) || clean_alpha_trim.starts_with("hi") && clean_alpha_trim.chars().all(|c| c == 'h' || c == 'i')
      || clean_alpha_trim.starts_with("hey") && clean_alpha_trim.chars().all(|c| c == 'h' || c == 'e' || c == 'y')
    {
        return false;
    }
    if task_lower.starts_with("what is ")
        && !task_lower.contains("file")
        && !task_lower.contains("code")
        && !task_lower.contains("repo")
    {
        return false;
    }
    if task_lower.starts_with("explain ")
        && !task_lower.contains("code")
        && !task_lower.contains("file")
        && !task_lower.contains("repo")
        && !task_lower.contains("fix")
        && !task_lower.contains("implement")
    {
        return false;
    }

    let keywords = [
        "create",
        "write",
        "edit",
        "modify",
        "add",
        "implement",
        "fix",
        "update",
        "delete",
        "file",
        "code",
        "refactor",
        "bug",
        "repo",
        "function",
        "class",
        "script",
        "test",
        "build",
        "change",
        "remove",
        "replace",
        "patch",
        "repair",
        "benchmark",
        "suite",
        "setup",
        "resolve",
        "correct",
        "correction",
        "investigate",
        "ensure",
        "invalidate",
        "handler",
        "auth",
        "token",
        "session",
        "logic",
        "issue",
        "module",
        "component",
        "trait",
        "struct",
        "enum",
        "package",
        "crate",
        "dependency",
        "api",
        "endpoint",
        "route",
        "pipeline",
        "workflow",
        "worker",
        "middleware",
        "router",
        "schema",
        "table",
        "migration",
        "query",
        "runner",
        "check",
        "verify",
        "assert",
        "performance",
        "perf",
        "leak",
        "cleanup",
        "optimize",
        "policy",
        "guard",
        "cache",
        "eviction",
        "storage",
        "lock",
        "deadlock",
        "mutex",
        "concurrency",
        "channel",
        "queue",
        "socket",
        "connection",
        "client",
        "server",
        "request",
        "response",
        "state",
        "store",
        "event",
        "listener",
        "stream",
        "buffer",
        "reader",
        "writer",
        "parser",
        "lexer",
        "encoder",
        "decoder",
        "serializer",
        "deserializer",
        "config",
        "setting",
        "options",
        "param",
        "parameter",
        "arg",
        "argument",
        "flag",
        "env",
        "environment",
        "secret",
        "key",
        "credential",
        "access",
        "role",
        "policy",
        "rule",
        "filter",
        "validator",
        "validation",
        "error",
        "exception",
        "failure",
        "panic",
        "crash",
        "hang",
        "timeout",
        "retry",
        "backoff",
        "circuit",
        "breaker",
        "rate",
        "limit",
        "throttle",
        "batch",
        "flush",
        "sync",
        "async",
        "future",
        "task",
        "job",
        "scheduler",
        "timer",
        "clock",
        "date",
        "timestamp",
        "log",
        "logger",
        "metric",
        "tracing",
        "trace",
        "telemetry",
        "monitor",
        "health",
        "probe",
        "signal",
        "hook",
        "callback",
        "plugin",
        "adapter",
        "proxy",
        "calculation",
        "compute",
        "precision",
        "overflow",
        "underflow",
        "memory",
        "leak",
    ];

    let has_keyword = keywords.iter().any(|kw| task_lower.contains(kw));
    let has_file_ext = task_lower.contains(".rs")
        || task_lower.contains(".py")
        || task_lower.contains(".toml")
        || task_lower.contains(".txt")
        || task_lower.contains(".js")
        || task_lower.contains(".ts")
        || task_lower.contains("/");

    has_keyword || has_file_ext
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_workspace_task_classification() {
        // Conversational / Q&A
        assert!(!is_workspace_task("heya"));
        assert!(!is_workspace_task("hello"));
        assert!(!is_workspace_task("hi"));
        assert!(!is_workspace_task("what is 1+1"));
        assert!(!is_workspace_task("explain quantum mechanics"));

        // Adversarial workspace tasks without obvious keywords
        assert!(is_workspace_task("The authentication flow allows expired sessions to remain valid. Investigate the implementation and make the necessary correction."));
        assert!(is_workspace_task(
            "The user balance calculation drops precision after division."
        ));
        assert!(is_workspace_task(
            "Cache eviction under high pressure causes memory leaks."
        ));
        assert!(is_workspace_task("src/main.rs"));
    }

    #[test]
    fn test_task_requirements_classification() {
        let req1 = classify_task_requirements("heya", &[]);
        assert!(!req1.requires_workspace);
        assert_eq!(
            select_execution_strategy(&req1),
            ExecutionStrategy::DirectResponse
        );

        let req2 = classify_task_requirements("Fix typo in README.txt", &[]);
        assert!(req2.requires_workspace);
        assert_eq!(req2.artifact_scope, ScopeType::SingleFile);
        assert_eq!(
            select_execution_strategy(&req2),
            ExecutionStrategy::SingleTurnEdit
        );

        let req3 = classify_task_requirements(
            "Refactor authentication module across repo and run test suite",
            &[],
        );
        assert!(req3.requires_workspace);
        assert_eq!(req3.artifact_scope, ScopeType::WorkspaceWide);
        assert!(req3.requires_verification);
        assert!(matches!(
            select_execution_strategy(&req3),
            ExecutionStrategy::CheckpointedLoop { .. }
        ));
    }

    #[test]
    fn test_from_checkpoint_preserves_budget_and_history() {
        let cp = TaskCheckpoint {
            task_id: "task_999".to_string(),
            goal: "Refactor auth engine".to_string(),
            phase: "verify".to_string(),
            verification_attempts: 3,
            executed_tools_summary: vec![
                "read_file (ok)".to_string(),
                "edit_file (ok)".to_string(),
            ],
            verification_failures: vec!["test failure in auth_test.rs".to_string()],
            completed_subtasks: vec!["subtask_1".to_string()],
            pending_subtasks: vec!["subtask_2".to_string()],
            modified_files: vec!["src/auth.rs".to_string()],
            visited_files: vec!["src/auth.rs".to_string(), "src/main.rs".to_string()],
            execution_strategy: Some("checkpointed:10:8".to_string()),
            telemetry: chilli_memory::CompactTaskTelemetry {
                input_tokens: 500,
                output_tokens: 200,
                cached_tokens: 100,
                total_tokens: 800,
                ..Default::default()
            },
            ..TaskCheckpoint::default()
        };

        let state = ActiveTaskState::from_checkpoint(&cp);

        assert_eq!(state.task_id.as_deref(), Some("task_999"));
        assert_eq!(state.goal, "Refactor auth engine");
        assert_eq!(state.phase, TaskPhase::Verify);
        assert_eq!(state.verification_attempts, 3);
        assert_eq!(state.executed_tools_summary.len(), 2);
        assert_eq!(state.verification_failures.len(), 1);
        assert_eq!(state.checkpoint.completed_subtasks, vec!["subtask_1"]);
        assert_eq!(state.checkpoint.pending_subtasks, vec!["subtask_2"]);
        assert_eq!(state.modified_files, vec!["src/auth.rs"]);
        assert_eq!(state.checkpoint.modified_files, vec!["src/auth.rs"]);
        assert_eq!(state.visited_files.len(), 2);
        assert_eq!(state.telemetry.input_tokens, 500);
        assert_eq!(state.telemetry.output_tokens, 200);
        assert_eq!(state.telemetry.cached_tokens, 100);
        assert_eq!(state.telemetry.total_task_tokens, 800);
        assert!(state.strategy.is_some());
    }

    #[test]
    fn test_try_from_checkpoint_validations() {
        let mut cp = TaskCheckpoint {
            task_id: "task_001".to_string(),
            goal: "Fix bug".to_string(),
            ..TaskCheckpoint::default()
        };

        // Valid checkpoint -> Ok
        assert!(ActiveTaskState::try_from_checkpoint(&cp).is_ok());

        // Invalid schema version -> Err
        cp.schema_version = 999;
        assert!(ActiveTaskState::try_from_checkpoint(&cp).is_err());
        cp.schema_version = CURRENT_SCHEMA_VERSION;

        // Empty task_id -> Err
        cp.task_id = "   ".to_string();
        assert!(ActiveTaskState::try_from_checkpoint(&cp).is_err());
        cp.task_id = "task_001".to_string();

        // Empty goal -> Err
        cp.goal = "".to_string();
        assert!(ActiveTaskState::try_from_checkpoint(&cp).is_err());
        cp.goal = "Fix bug".to_string();

        // Unparseable strategy -> Err
        cp.execution_strategy = Some("invalid_strategy_name_123".to_string());
        assert!(ActiveTaskState::try_from_checkpoint(&cp).is_err());
    }
}
