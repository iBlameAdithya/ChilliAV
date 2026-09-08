use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static OP_COUNTER: AtomicU64 = AtomicU64::new(1);
fn rand_counter() -> u64 {
    OP_COUNTER.fetch_add(1, Ordering::SeqCst)
}
use std::sync::Arc;

use chilli_context::builder::ContextBuilder;
use chilli_memory::journal::{EventJournal, EventRecord};
use chilli_memory::storage::StoragePaths;
use chilli_memory::{
    CurrentWorkspaceState, SafeResumeDecision, TaskCheckpoint, TaskRecoveryValidator,
    WorkspaceValidationResult,
};
use chilli_policy::path_policy::{PathPolicy, PolicyDecision};
use chilli_sandbox::sandbox::SandboxCapabilities;
use chilli_sandbox::worktree::{WorktreeGuard, WorktreeManager, WorktreeMode};
use chilli_tools::native_tools::{
    CopyFileTool, CreateDirectoryTool, EditFileTool, ExecTool, FindReferencesTool, FindSymbolTool,
    FindTestForFailureTool, GrepTool, ListDirectoryTool, MoveFileTool, ReadFileTool, RemoveFileTool,
    Tool, WriteFileTool,
};
use serde::{Deserialize, Serialize};

use chilli_model::adapter::{CompletionRequest, ModelAdapter};
use tokio_stream::StreamExt;
use tracing::debug;

use crate::loop_detector::{DuplicateLoopDetector, LoopAction};
use crate::state::{ActiveTaskState, AutonomyMode, EngineEvent, ExecutionState, TaskPhase};
use crate::verifier::{VerificationEngine, VerificationSpec};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MockResponse {
    ToolCall {
        name: String,
        arguments: serde_json::Value,
    },
    Text(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub completed: bool,
    pub final_output: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PromptProfile {
    Frontier,
    #[default]
    Standard,
    SmallModel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeResult {
    pub recovered: bool,
    pub last_checkpoint_id: String,
    pub replayed_events_count: usize,
    pub uncommitted_operations: Vec<String>,
}

pub struct AgentEngine {
    workspace_root: PathBuf,
    worktree_guard: WorktreeGuard,
    journal: EventJournal,
    policy: PathPolicy,
    sandbox_caps: SandboxCapabilities,
    read_tool: ReadFileTool,
    write_tool: WriteFileTool,
    edit_tool: EditFileTool,
    grep_tool: GrepTool,
    list_dir_tool: ListDirectoryTool,
    exec_tool: ExecTool,
    move_tool: MoveFileTool,
    copy_tool: CopyFileTool,
    create_dir_tool: CreateDirectoryTool,
    remove_tool: RemoveFileTool,
    find_symbol_tool: FindSymbolTool,
    find_references_tool: FindReferencesTool,
    find_test_for_failure_tool: FindTestForFailureTool,
    verifier: VerificationEngine,
    verification_spec: Option<VerificationSpec>,
    context_builder: ContextBuilder,
    pub codegraph: Option<chilli_repo_intel::repo_intel::CodeGraph>,
    mock_responses: Vec<MockResponse>,
    state: ExecutionState,
    pub active_task: Option<ActiveTaskState>,
    pub autonomy_mode: AutonomyMode,
    event_tx: Option<tokio::sync::mpsc::UnboundedSender<EngineEvent>>,
    control_rx: Option<tokio::sync::mpsc::UnboundedReceiver<crate::state::ControlEvent>>,
    cancel_flag: Arc<AtomicBool>,
    pub prompt_profile: PromptProfile,
    pub loop_detector: DuplicateLoopDetector,
    pub capability_profile: chilli_model::capabilities::ModelCapabilityProfile,
}

impl AgentEngine {
    pub fn new(workspace_root: &Path) -> Result<Self, String> {
        Self::new_with_worktree_mode(workspace_root, WorktreeMode::Direct)
    }

    pub fn new_with_worktree_mode(
        workspace_root: &Path,
        mode: WorktreeMode,
    ) -> Result<Self, String> {
        let worktree_guard = WorktreeManager::create(workspace_root, mode)?;
        let effective_root = worktree_guard.path().to_path_buf();

        let storage = StoragePaths::resolve(&effective_root)
            .map_err(|e| format!("Failed to init storage: {}", e))?;
        let journal = EventJournal::open(&storage.journal_db_path)
            .map_err(|e| format!("Failed to open journal: {}", e))?;
        let policy = PathPolicy;
        let sandbox_caps = SandboxCapabilities::detect();

        let read_tool = ReadFileTool::new(effective_root.clone());
        let write_tool = WriteFileTool::new(effective_root.clone());
        let edit_tool = EditFileTool::new(effective_root.clone());
        let grep_tool = GrepTool::new(effective_root.clone());
        let list_dir_tool = ListDirectoryTool::new(effective_root.clone());
        let exec_tool = ExecTool::new(effective_root.clone());
        let move_tool = MoveFileTool::new(effective_root.clone());
        let copy_tool = CopyFileTool::new(effective_root.clone());
        let create_dir_tool = CreateDirectoryTool::new(effective_root.clone());
        let remove_tool = RemoveFileTool::new(effective_root.clone());
        let find_symbol_tool = FindSymbolTool::new(effective_root.clone());
        let find_references_tool = FindReferencesTool::new(effective_root.clone());
        let find_test_for_failure_tool = FindTestForFailureTool::new(effective_root.clone());
        let verifier = VerificationEngine::new(&effective_root);

        let context_builder = ContextBuilder::new(100_000);

        Ok(Self {
            workspace_root: effective_root,
            worktree_guard,
            journal,
            policy,
            sandbox_caps,
            read_tool,
            write_tool,
            edit_tool,
            grep_tool,
            list_dir_tool,
            exec_tool,
            move_tool,
            copy_tool,
            create_dir_tool,
            remove_tool,
            find_symbol_tool,
            find_references_tool,
            find_test_for_failure_tool,
            verifier,
            verification_spec: None,
            context_builder,
            codegraph: None,
            mock_responses: Vec::new(),
            state: ExecutionState::Idle,
            active_task: None,
            autonomy_mode: AutonomyMode::Auto,
            event_tx: None,
            control_rx: None,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            prompt_profile: PromptProfile::Standard,
            loop_detector: DuplicateLoopDetector::new(),
            capability_profile: chilli_model::capabilities::ModelCapabilityProfile::new(
                "ollama",
                "qwen3:1.7b",
                8192,
            ),
        })
    }

    pub fn enable_codegraph(&mut self) -> Result<(), String> {
        let mut graph = chilli_repo_intel::repo_intel::CodeGraph::open_in_memory()?;
        graph.index_workspace(&self.workspace_root)?;
        self.codegraph = Some(graph);
        Ok(())
    }

    pub fn active_task(&self) -> Option<&ActiveTaskState> {
        self.active_task.as_ref()
    }

    pub fn active_task_mut(&mut self) -> Option<&mut ActiveTaskState> {
        self.active_task.as_mut()
    }

    pub fn set_event_sender(&mut self, tx: tokio::sync::mpsc::UnboundedSender<EngineEvent>) {
        self.event_tx = Some(tx);
    }

    pub fn clear_event_sender(&mut self) {
        self.event_tx = None;
    }

    pub fn set_control_receiver(
        &mut self,
        rx: tokio::sync::mpsc::UnboundedReceiver<crate::state::ControlEvent>,
    ) {
        self.control_rx = Some(rx);
    }

    pub fn clear_control_receiver(&mut self) {
        self.control_rx = None;
    }

    pub fn record_tool_failure_for_loop_detector(
        &mut self,
        tool: &str,
        args: &serde_json::Value,
        output: &str,
    ) -> LoopAction {
        self.loop_detector.record_failure(tool, args, output)
    }

    pub fn is_tool_call_locked(&self, _tool: &str, args: &serde_json::Value) -> bool {
        let hash = DuplicateLoopDetector::hash_args(args);
        self.loop_detector.is_locked(hash)
    }

    pub async fn receive_control_event(&mut self) -> Option<crate::state::ControlEvent> {
        if let Some(ref mut rx) = self.control_rx {
            rx.recv().await
        } else {
            None
        }
    }

    pub fn emit_event(&self, event: EngineEvent) {
        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(event);
        }
    }

    pub fn set_state(&mut self, state: ExecutionState) {
        self.state = state.clone();
        self.emit_event(EngineEvent::ExecutionStateChanged { state });
    }

    pub fn update_phase(&mut self, phase: TaskPhase) {
        if let Some(at) = self.active_task.as_mut() {
            at.phase = phase;
        }
        self.emit_event(EngineEvent::PhaseChanged { phase });
        if phase == TaskPhase::Plan && self.autonomy_mode == AutonomyMode::Plan {
            let summary = self
                .active_task
                .as_ref()
                .map(|t| format!("Plan generated for task: {}", t.goal))
                .unwrap_or_else(|| "Plan generated".to_string());
            self.emit_event(EngineEvent::PlanAwaitingApproval {
                plan_summary: summary,
            });
        }
    }

    pub fn set_autonomy_mode(&mut self, mode: AutonomyMode) {
        self.autonomy_mode = mode;
    }

    pub fn autonomy_mode(&self) -> AutonomyMode {
        self.autonomy_mode
    }

    pub fn set_prompt_profile(&mut self, profile: PromptProfile) {
        self.prompt_profile = profile;
    }

    pub fn worktree_path(&self) -> &Path {
        self.worktree_guard.path()
    }

    pub fn set_mock_responses(&mut self, responses: Vec<MockResponse>) {
        self.mock_responses = responses;
    }

    pub fn set_verification_spec(&mut self, spec: VerificationSpec) {
        self.verification_spec = Some(spec);
    }

    pub fn check_verification(&mut self) -> Result<Option<Vec<String>>, String> {
        let verification_enabled =
            crate::ABLATION_VERIFICATION.load(std::sync::atomic::Ordering::SeqCst);
        if !verification_enabled {
            return Ok(None);
        }
        if let Some(spec) = &self.verification_spec {
            let v_res = self.verifier.verify(spec);
            if !v_res.passed {
                let failure_reasons: Vec<String> =
                    v_res.failures.iter().map(|f| f.reason.clone()).collect();
                self.capability_profile.record_observation(
                    chilli_model::capabilities::ObservationSample {
                        tool_call_accuracy: 1.0,
                        instruction_following: 1.0,
                        verification_success: 0.0,
                        recovery_success: 0.0,
                    },
                );
                Ok(Some(failure_reasons))
            } else {
                self.capability_profile.record_observation(
                    chilli_model::capabilities::ObservationSample {
                        tool_call_accuracy: 1.0,
                        instruction_following: 1.0,
                        verification_success: 1.0,
                        recovery_success: 1.0,
                    },
                );
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    pub fn journal(&self) -> &EventJournal {
        &self.journal
    }

    pub fn state(&self) -> &ExecutionState {
        &self.state
    }

    pub fn sandbox_capabilities(&self) -> &SandboxCapabilities {
        &self.sandbox_caps
    }

    pub fn context_builder(&self) -> &ContextBuilder {
        &self.context_builder
    }

    pub fn context_builder_mut(&mut self) -> &mut ContextBuilder {
        &mut self.context_builder
    }

    pub fn cancel_handle(&self) -> Arc<AtomicBool> {
        self.cancel_flag.clone()
    }

    pub fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
    }

    pub async fn resume_task(&mut self) -> Result<ResumeResult, String> {
        let events = self
            .journal
            .events_after("default_session", 0)
            .map_err(|e| format!("Failed to read journal: {}", e))?;

        let mut last_checkpoint_id = String::new();
        let mut started_ops = std::collections::HashSet::new();
        let mut committed_ops = std::collections::HashSet::new();

        for event in &events {
            if event.event_type == "CheckpointSaved" {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&event.payload_json) {
                    if let Some(cp) = val.get("checkpoint_id").and_then(|v| v.as_str()) {
                        last_checkpoint_id = cp.to_string();
                    }
                }
            } else if event.event_type == "ToolCallStarted" {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&event.payload_json) {
                    if let Some(op_id) = val.get("op_id").and_then(|v| v.as_str()) {
                        started_ops.insert(op_id.to_string());
                    }
                }
            } else if event.event_type == "ToolCallCommitted"
                || event.event_type == "ToolResultRecorded"
                || event.event_type == "ToolExecuted"
            {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&event.payload_json) {
                    if let Some(op_id) = val.get("op_id").and_then(|v| v.as_str()) {
                        committed_ops.insert(op_id.to_string());
                    }
                }
            }
        }

        if last_checkpoint_id.is_empty() {
            return Ok(ResumeResult {
                recovered: false,
                last_checkpoint_id: "fallback_initial".to_string(),
                replayed_events_count: events.len(),
                uncommitted_operations: Vec::new(),
            });
        }

        let uncommitted_operations: Vec<String> =
            started_ops.difference(&committed_ops).cloned().collect();

        self.state = ExecutionState::Finished;
        Ok(ResumeResult {
            recovered: true,
            last_checkpoint_id,
            replayed_events_count: events.len(),
            uncommitted_operations,
        })
    }

    pub fn build_system_prompt(&self) -> String {
        let os_name = std::env::consts::OS;
        let os_arch = std::env::consts::ARCH;
        let root_str = self.workspace_root.to_string_lossy();
        match self.prompt_profile {
            PromptProfile::SmallModel => {
                format!(
                    "You are Chilli, a software engineering agent.\n\
                     Host Environment: OS={}, Arch={}, Root={}\n\
                     Execution Guidelines:\n\
                     - Use OS-appropriate paths and commands. On Windows, prefer native tools (list_directory, read_file, write_file, edit_file, grep) or native binaries.\n\
                     - For conversational inputs (greetings, chit-chat) or standalone general questions, reply directly in text.\n\
                     - When creating a NEW file, use write_file directly. Do not call edit_file on non-existent files.\n\
                     - For workspace engineering tasks (inspecting files, debugging, refactoring, building, or modifying code), call tools directly to perform work in the repository.\n\
                     - Execute tools to inspect files and verify changes before declaring a task complete.\n\
                     Tool Calling Instructions:\n\
                     To call a tool, use native tool function calls or format your request as a ```json markdown block containing standard tool JSON:\n\
                     ```json\n\
                     {{\"name\": \"tool_name\", \"arguments\": {{\"param\": \"value\"}}}}\n\
                     ```\n\
                     Supported tools: read_file, write_file, edit_file, grep, list_directory, exec, move_file, copy_file, create_directory, remove_file.",
                    os_name, os_arch, root_str
                )
            }
            PromptProfile::Standard | PromptProfile::Frontier => {
                format!(
                    "You are Chilli, a high-reliability AI software engineering agent.\n\
                     Host Environment:\n\
                     - OS: {}\n\
                     - Architecture: {}\n\
                     - Workspace Root: {}\n\
                     Execution Guidelines:\n\
                     - Use OS-appropriate paths and commands for {}. On Windows, prefer native tools (list_directory, read_file, write_file, edit_file, grep) or native binaries.\n\
                     - For all workspace engineering tasks (inspecting files, debugging issues, modifying code, building, or running tests), call tools directly to inspect or edit the repository.\n\
                     - When creating or writing a new file, call write_file directly.\n\
                     - Follow a disciplined engineering workflow: 1. Locate relevant code. 2. Inspect implementation. 3. Perform edits. 4. Verify outcomes.\n\
                     - Refactoring & Multi-file Execution Guidelines:\n\
                       1. Inspect item definition first.\n\
                       2. Search references and callers across workspace using grep.\n\
                       3. Inspect callers and tests.\n\
                       4. Perform edits on target files.\n\
                       5. Verify compilation and test suite.\n\
                     - For purely conversational inputs (greetings, chit-chat) or standalone arithmetic/general knowledge questions with no repository context, reply directly in text.",
                    os_name, os_arch, root_str, os_name
                )
            }
        }
    }

    pub fn default_tools_declaration() -> serde_json::Value {
        serde_json::json!([
            {
                "name": "read_file",
                "description": "Read contents of an existing file. Do not call read_file to create a new file; use write_file instead.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Relative file path" },
                        "offset": { "type": "integer", "description": "Optional starting line offset (1-based)" },
                        "limit": { "type": "integer", "description": "Optional line count limit" }
                    },
                    "required": ["path"]
                }
            },
            {
                "name": "write_file",
                "description": "Write or create a file with specified contents.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Relative file path" },
                        "content": { "type": "string", "description": "File content to write" }
                    },
                    "required": ["path", "content"]
                }
            },
            {
                "name": "edit_file",
                "description": "Edit file contents",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Relative file path" },
                        "old_string": { "type": "string", "description": "Exact text snippet to replace" },
                        "new_string": { "type": "string", "description": "New replacement text snippet" }
                    },
                    "required": ["path", "old_string", "new_string"]
                }
            },
            {
                "name": "grep",
                "description": "Search pattern in files",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string", "description": "Search pattern" }
                    },
                    "required": ["pattern"]
                }
            },
            {
                "name": "list_directory",
                "description": "List files and directories at a given relative path",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Optional relative directory path (defaults to '.')" }
                    }
                }
            },
            {
                "name": "exec",
                "description": "Execute shell process command. Pass executable or full command string (e.g. 'python script.py') in 'command'.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "Shell command executable or full command line" },
                        "args": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optional command arguments"
                        },
                        "cwd": { "type": "string", "description": "Optional relative working directory" },
                        "timeout_ms": { "type": "integer", "description": "Optional timeout in milliseconds" }
                    },
                    "required": ["command"]
                }
            },
            {
                "name": "move_file",
                "description": "Move or rename a file or directory",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "from": { "type": "string", "description": "Source path relative to workspace root" },
                        "to": { "type": "string", "description": "Destination path relative to workspace root" }
                    },
                    "required": ["from", "to"]
                }
            },
            {
                "name": "copy_file",
                "description": "Copy a file or directory to a new location",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "from": { "type": "string", "description": "Source path relative to workspace root" },
                        "to": { "type": "string", "description": "Destination path relative to workspace root" }
                    },
                    "required": ["from", "to"]
                }
            },
            {
                "name": "create_directory",
                "description": "Create a directory (and any necessary parent directories)",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Relative directory path to create" }
                    },
                    "required": ["path"]
                }
            },
            {
                "name": "remove_file",
                "description": "Remove a file or directory from the workspace",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Relative path of file or directory to remove" }
                    },
                    "required": ["path"]
                }
            },
            {
                "name": "find_symbol",
                "description": "Find symbol, function, or class definition across the workspace",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Symbol, function, or class name to locate" }
                    },
                    "required": ["name"]
                }
            },
            {
                "name": "find_references",
                "description": "Find usages and references to a symbol across the workspace",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "symbol": { "type": "string", "description": "Symbol or identifier name to search references for" }
                    },
                    "required": ["symbol"]
                }
            },
            {
                "name": "find_test_for_failure",
                "description": "Locate and inspect failing test file source and assertion block from an execution traceback",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "traceback": { "type": "string", "description": "Failure traceback text containing file paths and test names" }
                    },
                    "required": ["traceback"]
                }
            }
        ])
    }

    pub fn tools_declaration_for_phase(_phase: TaskPhase) -> serde_json::Value {
        Self::default_tools_declaration()
    }

    pub fn validate_checkpoint_resume(
        &self,
        checkpoint: &TaskCheckpoint,
    ) -> WorkspaceValidationResult {
        let current_workspace = CurrentWorkspaceState::inspect(&self.workspace_root);
        TaskRecoveryValidator::validate(checkpoint, &current_workspace)
    }

    pub fn evaluate_checkpoint_recovery_gate(
        &self,
        checkpoint: &TaskCheckpoint,
    ) -> Result<WorkspaceValidationResult, String> {
        if checkpoint.status == chilli_memory::TaskStatus::Completed
            || checkpoint.phase.eq_ignore_ascii_case("completed")
        {
            return Err("Cannot resume completed task".to_string());
        }

        let val_res = self.validate_checkpoint_resume(checkpoint);

        match &val_res.decision {
            SafeResumeDecision::SafeToResume => Ok(val_res),
            SafeResumeDecision::SafeWithNotice { .. } => Ok(val_res),
            SafeResumeDecision::ExternalChangesDetected {
                conflicting_files, ..
            } => Err(format!(
                "Recovery blocked: external modifications detected in visited/modified files: {:?}",
                conflicting_files
            )),
            SafeResumeDecision::BranchChanged {
                checkpoint_branch,
                current_branch,
            } => Err(format!(
                "Recovery blocked: branch changed from {:?} to {:?}",
                checkpoint_branch, current_branch
            )),
            SafeResumeDecision::WorkspaceMissing { expected_root } => Err(format!(
                "Recovery blocked: workspace root missing or inaccessible: {}",
                expected_root
            )),
            SafeResumeDecision::RepositoryChanged {
                checkpoint_commit,
                current_commit,
            } => Err(format!(
                "Recovery blocked: repository commit changed from {:?} to {:?}",
                checkpoint_commit, current_commit
            )),
            SafeResumeDecision::CheckpointStale {
                age_secs,
                max_allowed_secs,
            } => Err(format!(
                "Recovery blocked: checkpoint stale (age: {}s, max allowed: {}s)",
                age_secs, max_allowed_secs
            )),
            SafeResumeDecision::CheckpointCorrupt { details } => Err(format!(
                "Recovery blocked: corrupt checkpoint data: {}",
                details
            )),
            SafeResumeDecision::UnsupportedWorkspace { reason } => Err(format!(
                "Recovery blocked: unsupported workspace: {}",
                reason
            )),
            SafeResumeDecision::UnsafeToResume { reasons } => Err(format!(
                "Recovery blocked: unsafe to resume: {}",
                reasons.join(", ")
            )),
        }
    }

    pub fn setup_resumed_task(
        &mut self,
        checkpoint: &TaskCheckpoint,
        notices: &[String],
    ) -> Result<(), String> {
        let reconstructed_state = ActiveTaskState::from_checkpoint(checkpoint);
        self.active_task = Some(reconstructed_state.clone());
        self.state = ExecutionState::AssemblingContext;

        let sys_prompt = self.build_system_prompt();
        self.context_builder.set_system_prompt(&sys_prompt);
        self.context_builder
            .set_tools_declaration(Self::tools_declaration_for_phase(reconstructed_state.phase));
        self.context_builder
            .add_resumed_task_context(checkpoint, notices);
        let _ = self.context_builder.build_cache_aligned();

        Ok(())
    }

    pub async fn resume_task_from_checkpoint(
        &mut self,
        checkpoint: &TaskCheckpoint,
    ) -> Result<TaskResult, String> {
        if self.mock_responses.is_empty() {
            let default_adapter = chilli_model::create_adapter_for_model(None);
            return self
                .resume_task_from_checkpoint_with_adapter(checkpoint, default_adapter)
                .await;
        }

        let val_res = match self.evaluate_checkpoint_recovery_gate(checkpoint) {
            Ok(res) => res,
            Err(e) => {
                if e.contains("Cannot resume completed task") {
                    return Err(e);
                }
                let fallback_notice =
                    format!("Resume gate notice: {}. Falling back to clean state.", e);
                let mut fallback_checkpoint = checkpoint.clone();
                fallback_checkpoint.status = chilli_memory::TaskStatus::Active;
                fallback_checkpoint.updated_at = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                self.setup_resumed_task(&fallback_checkpoint, &[fallback_notice])?;
                return Box::pin(self.resume_task_from_checkpoint(&fallback_checkpoint)).await;
            }
        };

        let notices = match &val_res.decision {
            SafeResumeDecision::SafeWithNotice { notices } => notices.clone(),
            _ => Vec::new(),
        };

        self.setup_resumed_task(checkpoint, &notices)?;

        let mut zero_tool_retries = if self
            .active_task
            .as_ref()
            .map(|t| t.zero_tool_guard_triggered)
            .unwrap_or(false)
        {
            1
        } else {
            0
        };

        while !self.mock_responses.is_empty() {
            if self.cancel_flag.load(Ordering::SeqCst) {
                self.state = ExecutionState::Failed;
                if let Some(at) = self.active_task.as_mut() {
                    at.phase = TaskPhase::Failed;
                }
                return Err("Execution cancelled".to_string());
            }

            let current_phase = self
                .active_task
                .as_ref()
                .map(|t| t.phase)
                .unwrap_or(TaskPhase::Understand);
            self.context_builder
                .set_tools_declaration(Self::tools_declaration_for_phase(current_phase));

            self.state = ExecutionState::RequestingModel;
            let mock_resp = self.mock_responses.remove(0);

            match mock_resp {
                MockResponse::ToolCall { name, arguments } => {
                    self.state = ExecutionState::ParsingToolCalls;
                    self.state = ExecutionState::EvaluatingPolicy;

                    if let Some(at) = self.active_task.as_mut() {
                        at.executed_tools_summary.push(name.clone());
                        if name == "write_file" || name == "edit_file" {
                            if let Some(path_str) = arguments.get("path").and_then(|p| p.as_str()) {
                                at.record_file_modification(path_str);
                            }
                            at.phase = TaskPhase::Execute;
                        } else if at.phase == TaskPhase::Understand {
                            at.phase = TaskPhase::Inspect;
                        }
                    }

                    let op_id = format!(
                        "op_{}_{}",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_nanos(),
                        rand_counter()
                    );

                    let mut decision = if name == "exec" {
                        let cmd_str = arguments
                            .get("command")
                            .and_then(|c| c.as_str())
                            .unwrap_or("");
                        let cwd_str = arguments.get("cwd").and_then(|c| c.as_str()).unwrap_or(".");
                        let target_cwd = self.workspace_root.join(cwd_str);
                        self.policy
                            .check_command(&self.workspace_root, &target_cwd, cmd_str)
                    } else if name == "write_file" || name == "edit_file" {
                        let path_arg = arguments.get("path").and_then(|p| p.as_str()).unwrap_or("");
                        let target_path = self.workspace_root.join(path_arg);
                        self.policy.check_write(&self.workspace_root, &target_path)
                    } else {
                        let path_arg = arguments.get("path").and_then(|p| p.as_str()).unwrap_or("");
                        let target_path = self.workspace_root.join(path_arg);
                        self.policy.check_read(&self.workspace_root, &target_path)
                    };

                    if self.autonomy_mode == AutonomyMode::Manual
                        && matches!(decision, PolicyDecision::Allow)
                    {
                        decision = PolicyDecision::Ask(format!(
                            "Manual mode authorization required for tool '{}'",
                            name
                        ));
                    }

                    if let PolicyDecision::Deny(reason) | PolicyDecision::Ask(reason) = decision {
                        self.emit_event(EngineEvent::ToolCallDenied {
                            tool_name: name.clone(),
                            reason: reason.clone(),
                        });
                        let _ = self.journal.append(&EventRecord {
                            seq: 0,
                            session_id: "default_session".to_string(),
                            event_type: "ToolDenied".to_string(),
                            payload_json: serde_json::json!({
                                "op_id": op_id,
                                "tool": name,
                                "error": reason,
                            })
                            .to_string(),
                            timestamp_ms: 0,
                        });
                        continue;
                    }

                    self.state = ExecutionState::ExecutingTools;
                    let tool_result =
                        self.execute_tool(&name, arguments.clone())
                            .unwrap_or_else(|e| chilli_tools::native_tools::ToolResult {
                                success: false,
                                output: format!("Tool execution error: {}", e),
                            });

                    let _ = self.journal.append(&EventRecord {
                        seq: 0,
                        session_id: "default_session".to_string(),
                        event_type: "ToolCallCommitted".to_string(),
                        payload_json: serde_json::json!({
                            "op_id": op_id,
                            "tool": name,
                        })
                        .to_string(),
                        timestamp_ms: 0,
                    });

                    let _ = self.journal.append(&EventRecord {
                        seq: 0,
                        session_id: "default_session".to_string(),
                        event_type: "ToolResultRecorded".to_string(),
                        payload_json: serde_json::json!({
                            "op_id": op_id,
                            "tool": name,
                            "success": tool_result.success,
                            "output": tool_result.output,
                        })
                        .to_string(),
                        timestamp_ms: 0,
                    });

                    let _ = self.journal.append(&EventRecord {
                        seq: 0,
                        session_id: "default_session".to_string(),
                        event_type: "ToolExecuted".to_string(),
                        payload_json: serde_json::json!({
                            "op_id": op_id,
                            "tool": name,
                            "arguments": arguments,
                            "output": tool_result.output,
                        })
                        .to_string(),
                        timestamp_ms: 0,
                    });

                    self.state = ExecutionState::RecordingObservations;
                    let obs_tokens = chilli_context::types::estimate_tokens(&tool_result.output);
                    if let Some(at) = self.active_task.as_mut() {
                        at.telemetry.tool_output_tokens += obs_tokens;
                    }
                    self.context_builder.add_tool_observation_with_id(
                        "call_mock",
                        &name,
                        &tool_result.output,
                    );
                }
                MockResponse::Text(text) => {
                    self.state = ExecutionState::CheckingCompletion;

                    let executed_tools_count = self
                        .active_task
                        .as_ref()
                        .map(|t| t.executed_tools_summary.len())
                        .unwrap_or(0);

                    if executed_tools_count == 0 && zero_tool_retries < 2 {
                        let is_ws = self
                            .active_task
                            .as_ref()
                            .map(|t| t.is_workspace_task())
                            .unwrap_or_else(|| crate::state::is_workspace_task(&checkpoint.goal));
                        if is_ws {
                            zero_tool_retries += 1;
                            if let Some(at) = self.active_task.as_mut() {
                                at.zero_tool_guard_triggered = true;
                            }
                            let feedback_msg = format!(
                                "Zero tools were invoked for workspace task (attempt {}/2). Please use file/exec tools to inspect or complete task.",
                                zero_tool_retries
                            );
                            self.context_builder
                                .add_message(chilli_context::types::Role::User, &feedback_msg);
                            continue;
                        }
                    }

                    if let Some(failure_reasons) = self.check_verification()? {
                        let _ = self.journal.append(&EventRecord {
                            seq: 0,
                            session_id: "default_session".to_string(),
                            event_type: "VerificationFailed".to_string(),
                            payload_json: serde_json::json!({
                                "failures": failure_reasons,
                            })
                            .to_string(),
                            timestamp_ms: 0,
                        });

                        let attempts = self
                            .active_task
                            .as_ref()
                            .map(|t| t.verification_attempts)
                            .unwrap_or(0);

                        if attempts < 3 {
                            let feedback_msg = self.build_strategy_progression_feedback(attempts, &failure_reasons);
                            if let Some(at) = self.active_task.as_mut() {
                                at.verification_attempts += 1;
                                at.consecutive_failures += 1;
                                at.phase = TaskPhase::Recover;
                                at.verification_failures.extend(failure_reasons.clone());
                            }
                            self.context_builder
                                .add_message(chilli_context::types::Role::User, &feedback_msg);
                            continue;
                        }

                        if let Some(at) = self.active_task.as_mut() {
                            at.phase = TaskPhase::Failed;
                        }
                        self.state = ExecutionState::Failed;
                        return Ok(TaskResult {
                            completed: false,
                            final_output: format!(
                                "Verification failed: {}",
                                failure_reasons.join("; ")
                            ),
                        });
                    }

                    if let Some(at) = self.active_task.as_mut() {
                        at.phase = TaskPhase::Finished;
                    }
                    self.state = ExecutionState::Finished;

                    let _ = self.journal.append(&EventRecord {
                        seq: 0,
                        session_id: "default_session".to_string(),
                        event_type: "TaskCompleted".to_string(),
                        payload_json: serde_json::json!({
                            "final_output": text,
                        })
                        .to_string(),
                        timestamp_ms: 0,
                    });

                    return Ok(TaskResult {
                        completed: true,
                        final_output: text,
                    });
                }
            }
        }

        if let Some(failure_reasons) = self.check_verification()? {
            let _ = self.journal.append(&EventRecord {
                seq: 0,
                session_id: "default_session".to_string(),
                event_type: "VerificationFailed".to_string(),
                payload_json: serde_json::json!({
                    "failures": failure_reasons,
                })
                .to_string(),
                timestamp_ms: 0,
            });
            self.state = ExecutionState::Failed;
            return Ok(TaskResult {
                completed: false,
                final_output: format!("Verification failed: {}", failure_reasons.join("; ")),
            });
        }

        self.state = ExecutionState::Finished;
        if let Some(at) = self.active_task.as_mut() {
            at.phase = TaskPhase::Finished;
        }
        Ok(TaskResult {
            completed: true,
            final_output: "Finished resumed execution.".to_string(),
        })
    }

    pub async fn resume_task_from_checkpoint_with_adapter(
        &mut self,
        checkpoint: &TaskCheckpoint,
        adapter: Arc<dyn ModelAdapter>,
    ) -> Result<TaskResult, String> {
        let val_res = match self.evaluate_checkpoint_recovery_gate(checkpoint) {
            Ok(res) => res,
            Err(e) => {
                if e.contains("Cannot resume completed task") {
                    return Err(e);
                }
                let fallback_notice =
                    format!("Resume gate notice: {}. Falling back to clean state.", e);
                let mut fallback_checkpoint = checkpoint.clone();
                fallback_checkpoint.status = chilli_memory::TaskStatus::Active;
                fallback_checkpoint.updated_at = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                self.setup_resumed_task(&fallback_checkpoint, &[fallback_notice])?;
                return self
                    .run_task_with_adapter(&fallback_checkpoint.goal, adapter)
                    .await;
            }
        };

        let notices = match &val_res.decision {
            SafeResumeDecision::SafeWithNotice { notices } => notices.clone(),
            _ => Vec::new(),
        };

        let model_name_lower = adapter.model_name().to_lowercase();
        if self.prompt_profile == PromptProfile::Standard
            && (model_name_lower.contains("llama")
                || model_name_lower.contains("70b")
                || model_name_lower.contains("8b")
                || model_name_lower.contains("qwen")
                || model_name_lower.contains("haiku")
                || model_name_lower.contains("groq"))
        {
            self.prompt_profile = PromptProfile::SmallModel;
        }

        self.setup_resumed_task(checkpoint, &notices)?;

        let task_goal = checkpoint.goal.clone();
        let mut accumulated_text = String::new();
        let mut max_turns = self
            .active_task
            .as_ref()
            .and_then(|at| at.strategy.as_ref())
            .map(|s| s.max_turns())
            .unwrap_or(10);
        let mut turn = 0;
        let mut elastic_extended = false;
        let mut executed_tools_summary: Vec<String> = self
            .active_task
            .as_ref()
            .map(|at| at.executed_tools_summary.clone())
            .unwrap_or_default();
        let mut empty_turns_count = 0;
        let mut consecutive_tool_failures = self
            .active_task
            .as_ref()
            .map(|at| at.consecutive_failures)
            .unwrap_or(0);
        let mut last_failed_call: Option<(String, serde_json::Value)> = None;
        let mut zero_tool_retries = if self
            .active_task
            .as_ref()
            .map(|t| t.zero_tool_guard_triggered)
            .unwrap_or(false)
        {
            1
        } else {
            0
        };

        while turn < max_turns {
            turn += 1;
            if self.cancel_flag.load(Ordering::SeqCst) {
                self.state = ExecutionState::Failed;
                if let Some(at) = self.active_task.as_mut() {
                    at.phase = TaskPhase::Failed;
                }
                debug!("[engine telemetry] state: {:?}", self.state);
                return Err("Execution cancelled".to_string());
            }

            let current_phase = self
                .active_task
                .as_ref()
                .map(|t| t.phase)
                .unwrap_or(TaskPhase::Understand);
            self.context_builder
                .set_tools_declaration(Self::tools_declaration_for_phase(current_phase));

            if let Some(at) = &self.active_task {
                let wm_summary = at.render_working_memory_summary();
                self.context_builder.set_checkpoint_summary(&wm_summary);
            }

            if turn == max_turns && !elastic_extended {
                if let Some(at) = &self.active_task {
                    if let Some(crate::state::ExecutionStrategy::CheckpointedLoop {
                        elastic_extension,
                        ..
                    }) = at.strategy
                    {
                        if !at.modified_files.is_empty() || at.verification_attempts > 0 {
                            elastic_extended = true;
                            max_turns += elastic_extension;
                            debug!(
                                "[engine telemetry] Elastic turn budget extended: +{} turns (new limit: {})",
                                elastic_extension, max_turns
                            );
                        }
                    }
                }
            }

            self.state = ExecutionState::RequestingModel;
            debug!("[engine telemetry] state: {:?}", self.state);

            let openai_msgs = self.context_builder.build_openai_messages();
            let formatted = self.context_builder.build().unwrap_or_else(|_| {
                chilli_context::types::FormattedPrompt {
                    system_prompt: "You are Chilli AI agent runtime.".to_string(),
                    tools_json: serde_json::json!([]),
                    messages: vec![],
                    estimated_tokens: 0,
                }
            });

            let prompt = formatted
                .messages
                .last()
                .map(|m| m.content.clone())
                .unwrap_or_else(|| task_goal.clone());
            let req = CompletionRequest {
                prompt,
                system_prompt: Some(formatted.system_prompt),
                tools_json: Some(formatted.tools_json),
                messages: if openai_msgs.is_empty() {
                    None
                } else {
                    Some(openai_msgs)
                },
                max_tokens: None,
                reasoning_effort: None,
            };

            let mut stream = adapter.complete(req).await?;
            let mut accumulator = crate::stream_accumulator::StreamAccumulator::new();

            while let Some(event) = stream.next().await {
                if let chilli_model::adapter::StreamEvent::TextChunk(ref delta) = event {
                    self.emit_event(EngineEvent::ModelStreamChunk {
                        delta: delta.clone(),
                    });
                }
                accumulator.feed(&event);
            }

            let _ = self.journal.append(&EventRecord {
                seq: 0,
                session_id: "default_session".to_string(),
                event_type: "ModelTurnCompleted".to_string(),
                payload_json: serde_json::json!({
                    "prompt_tokens": accumulator.token_usage.prompt_tokens,
                    "completion_tokens": accumulator.token_usage.completion_tokens,
                    "cached_tokens": accumulator.token_usage.cached_tokens,
                })
                .to_string(),
                timestamp_ms: 0,
            });

            let turn_text = accumulator.accumulated_text().to_string();
            accumulated_text.push_str(&turn_text);

            let tool_calls = accumulator.finish_tool_calls().unwrap_or_default();

            if !tool_calls.is_empty() {
                empty_turns_count = 0;
                self.set_state(ExecutionState::ParsingToolCalls);
                debug!("[engine telemetry] state: {:?}", self.state);

                let formatted_tool_calls: Vec<serde_json::Value> = tool_calls
                    .iter()
                    .enumerate()
                    .map(|(i, tc)| {
                        let id = if tc.id.is_empty() {
                            format!("call_{}", i)
                        } else {
                            tc.id.clone()
                        };
                        serde_json::json!({
                            "id": id,
                            "type": "function",
                            "function": {
                                "name": tc.name,
                                "arguments": tc.raw_arguments_json
                            }
                        })
                    })
                    .collect();

                self.context_builder.add_assistant_message(
                    &turn_text,
                    Some(serde_json::json!(formatted_tool_calls)),
                );

                for (i, tool_call) in tool_calls.into_iter().enumerate() {
                    let name = tool_call.name;
                    let args_val = tool_call.arguments;
                    let call_id = if tool_call.id.is_empty() {
                        format!("call_{}", i)
                    } else {
                        tool_call.id
                    };

                    self.set_state(ExecutionState::EvaluatingPolicy);
                    debug!("[engine telemetry] state: {:?}", self.state);
                    let op_id = format!(
                        "op_{}_{}",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_nanos(),
                        rand_counter()
                    );

                    let mut decision = if name == "exec" {
                        let cmd_str = args_val
                            .get("command")
                            .and_then(|c| c.as_str())
                            .unwrap_or("");
                        let cwd_str = args_val.get("cwd").and_then(|c| c.as_str()).unwrap_or(".");
                        let target_cwd = self.workspace_root.join(cwd_str);
                        self.policy
                            .check_command(&self.workspace_root, &target_cwd, cmd_str)
                    } else if name == "write_file" || name == "edit_file" {
                        let path_arg = args_val.get("path").and_then(|p| p.as_str()).unwrap_or("");
                        let target_path = self.workspace_root.join(path_arg);
                        self.policy.check_write(&self.workspace_root, &target_path)
                    } else {
                        let path_arg = args_val.get("path").and_then(|p| p.as_str()).unwrap_or("");
                        let target_path = self.workspace_root.join(path_arg);
                        self.policy.check_read(&self.workspace_root, &target_path)
                    };

                    if self.autonomy_mode == AutonomyMode::Manual
                        && matches!(decision, PolicyDecision::Allow)
                    {
                        decision = PolicyDecision::Ask(format!(
                            "Manual mode authorization required for tool '{}'",
                            name
                        ));
                    }

                    if let PolicyDecision::Deny(reason) | PolicyDecision::Ask(reason) = decision {
                        self.emit_event(EngineEvent::ToolCallDenied {
                            tool_name: name.clone(),
                            reason: reason.clone(),
                        });
                        let _ = self.journal.append(&EventRecord {
                            seq: 0,
                            session_id: "default_session".to_string(),
                            event_type: "ToolDenied".to_string(),
                            payload_json: serde_json::json!({
                                "op_id": op_id,
                                "tool": name,
                                "error": reason,
                            })
                            .to_string(),
                            timestamp_ms: 0,
                        });
                        let err_msg = format!("Policy Denied: {}", reason);
                        executed_tools_summary.push(format!("Tool {} denied: {}", name, reason));
                        self.context_builder
                            .add_tool_observation_with_id(&call_id, &name, &err_msg);
                        continue;
                    }

                    if self.is_tool_call_locked(&name, &args_val) {
                        let loop_act = self.record_tool_failure_for_loop_detector(
                            &name,
                            &args_val,
                            "Tool call signature locked",
                        );
                        if loop_act.is_terminate() {
                            let err_msg = format!("HARD INTERVENTION: Tool call signature for '{}' is locked due to duplicate failure loop (N>=4). Task terminated.", name);
                            self.context_builder
                                .add_tool_observation_with_id(&call_id, &name, &err_msg);
                            self.state = ExecutionState::Failed;
                            if let Some(at) = self.active_task.as_mut() {
                                at.phase = crate::state::TaskPhase::Failed;
                            }
                            return Ok(TaskResult {
                                completed: false,
                                final_output: format!(
                                    "Failure budget exceeded: duplicate failure loop on tool '{}'.",
                                    name
                                ),
                            });
                        }

                        let err_msg = format!("HARD INTERVENTION: Tool call signature for '{}' is locked due to duplicate failure loop (N>=3). You MUST alter parameters, try a different tool, or perform inspection.", name);
                        executed_tools_summary
                            .push(format!("Tool {} locked: duplicate loop", name));
                        self.context_builder
                            .add_tool_observation_with_id(&call_id, &name, &err_msg);
                        continue;
                    }

                    self.emit_event(EngineEvent::ToolCallStarted {
                        tool_name: name.clone(),
                        args_json: args_val.to_string(),
                    });
                    let _ = self.journal.append(&EventRecord {
                        seq: 0,
                        session_id: "default_session".to_string(),
                        event_type: "ToolCallStarted".to_string(),
                        payload_json: serde_json::json!({
                            "op_id": op_id,
                            "tool": name,
                            "arguments": args_val,
                        })
                        .to_string(),
                        timestamp_ms: 0,
                    });

                    self.set_state(ExecutionState::ExecutingTools);
                    debug!("[engine telemetry] state: {:?}", self.state);
                    let start_t = std::time::Instant::now();
                    let tool_result =
                        self.execute_tool(&name, args_val.clone())
                            .unwrap_or_else(|e| chilli_tools::native_tools::ToolResult {
                                success: false,
                                output: format!("Tool execution error: {}", e),
                            });
                    let dur = start_t.elapsed().as_millis() as u64;
                    self.emit_event(EngineEvent::ToolExecutionCompleted {
                        tool_name: name.clone(),
                        result_summary: tool_result.output.clone(),
                        duration_ms: dur,
                    });

                    let _ = self.journal.append(&EventRecord {
                        seq: 0,
                        session_id: "default_session".to_string(),
                        event_type: "ToolCallCommitted".to_string(),
                        payload_json: serde_json::json!({
                            "op_id": op_id,
                            "tool": name,
                        })
                        .to_string(),
                        timestamp_ms: 0,
                    });

                    let _ = self.journal.append(&EventRecord {
                        seq: 0,
                        session_id: "default_session".to_string(),
                        event_type: "ToolResultRecorded".to_string(),
                        payload_json: serde_json::json!({
                            "op_id": op_id,
                            "tool": name,
                            "success": tool_result.success,
                            "output": tool_result.output,
                        })
                        .to_string(),
                        timestamp_ms: 0,
                    });

                    let _ = self.journal.append(&EventRecord {
                        seq: 0,
                        session_id: "default_session".to_string(),
                        event_type: "ToolExecuted".to_string(),
                        payload_json: serde_json::json!({
                            "op_id": op_id,
                            "tool": name,
                            "arguments": args_val,
                            "output": tool_result.output,
                        })
                        .to_string(),
                        timestamp_ms: 0,
                    });

                    self.set_state(ExecutionState::RecordingObservations);
                    debug!("[engine telemetry] state: {:?}", self.state);

                    let is_dup_failure = if !tool_result.success {
                        if let Some((ref last_name, ref last_args)) = last_failed_call {
                            last_name == &name && last_args == &args_val
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    if tool_result.success {
                        self.loop_detector.record_success(&name, &args_val);
                        self.capability_profile.record_observation(
                            chilli_model::capabilities::ObservationSample {
                                tool_call_accuracy: 1.0,
                                instruction_following: 1.0,
                                verification_success: 1.0,
                                recovery_success: 1.0,
                            },
                        );
                        consecutive_tool_failures = 0;
                        last_failed_call = None;
                        executed_tools_summary.push(format!(
                            "Executed {} successfully: {}",
                            name,
                            tool_result.output.trim()
                        ));
                        if let Some(at) = self.active_task.as_mut() {
                            at.executed_tools_summary.push(format!("{} (ok)", name));
                            at.consecutive_failures = 0;
                            if let Some(path_str) = args_val.get("path").and_then(|p| p.as_str()) {
                                at.record_file_visit(path_str);
                            }
                            if name == "write_file" || name == "edit_file" {
                                if let Some(path_str) =
                                    args_val.get("path").and_then(|p| p.as_str())
                                {
                                    at.record_file_modification(path_str);
                                }
                                at.phase = TaskPhase::Execute;
                            } else if at.phase == TaskPhase::Understand {
                                at.phase = TaskPhase::Inspect;
                            }
                        }
                        self.context_builder.add_tool_observation_with_id(
                            &call_id,
                            &name,
                            &tool_result.output,
                        );
                    } else {
                        self.capability_profile.record_observation(
                            chilli_model::capabilities::ObservationSample {
                                tool_call_accuracy: 0.0,
                                instruction_following: 0.5,
                                verification_success: 0.0,
                                recovery_success: 0.0,
                            },
                        );
                        consecutive_tool_failures += 1;
                        last_failed_call = Some((name.clone(), args_val.clone()));

                        let loop_act = self.record_tool_failure_for_loop_detector(
                            &name,
                            &args_val,
                            &tool_result.output,
                        );
                        if loop_act.is_lock() {
                            self.prompt_profile = PromptProfile::Frontier;
                        }

                        let mut obs_output = tool_result.output.clone();
                        if loop_act.is_lock() {
                            self.prompt_profile = PromptProfile::Frontier;
                            obs_output.push_str(
                                "\n[HARD INTERVENTION: Duplicate failure threshold reached (N=3). \
                                 You MUST STOP repeating similar edits. Pause, perform a fresh diagnosis from scratch, \
                                 inspect actual file contents with read_file, and verify your assumptions before modifying files again.]"
                            );
                        } else if is_dup_failure {
                            obs_output.push_str(
                                "\n[SYSTEM NOTICE: Identical tool call failed again. Do NOT retry the exact same parameters without modifying your approach.]"
                            );
                        }

                        if loop_act.is_terminate() || consecutive_tool_failures >= 5 {
                            obs_output.push_str("\n[SYSTEM NOTICE: Task terminated due to duplicate failure loop (N=4) or failure budget.]");
                            self.context_builder.add_tool_observation_with_id(
                                &call_id,
                                &name,
                                &obs_output,
                            );
                            self.state = ExecutionState::Failed;
                            if let Some(at) = self.active_task.as_mut() {
                                at.phase = crate::state::TaskPhase::Failed;
                            }
                            return Ok(TaskResult {
                                completed: false,
                                final_output: format!("Task terminated: duplicate failure loop or budget exceeded on tool '{}'.", name),
                            });
                        }

                        executed_tools_summary.push(format!(
                            "Tool {} failed: {}",
                            name,
                            tool_result.output.trim()
                        ));
                        if let Some(at) = self.active_task.as_mut() {
                            at.executed_tools_summary.push(format!("{} (err)", name));
                            at.consecutive_failures = consecutive_tool_failures;
                        }
                        self.context_builder.add_tool_observation_with_id(
                            &call_id,
                            &name,
                            &obs_output,
                        );
                    }
                }
            } else {
                if executed_tools_summary.is_empty() && zero_tool_retries < 2 {
                    let is_ws = self
                        .active_task
                        .as_ref()
                        .map(|t| t.is_workspace_task())
                        .unwrap_or_else(|| crate::state::is_workspace_task(&task_goal));
                    if is_ws {
                        zero_tool_retries += 1;
                        if let Some(at) = self.active_task.as_mut() {
                            at.zero_tool_guard_triggered = true;
                        }
                        if !turn_text.trim().is_empty() {
                            self.context_builder.add_assistant_message(&turn_text, None);
                        }
                        self.context_builder.add_message(
                            chilli_context::types::Role::User,
                            "Task requires modifying repository files or executing commands, but zero tools were called. Inspect the workspace or make the required edit using tools before completing."
                        );
                        continue;
                    }
                }

                if turn_text.trim().is_empty() {
                    empty_turns_count += 1;
                    if !executed_tools_summary.is_empty() {
                        let summary = format!(
                            "Completed task actions:\n{}",
                            executed_tools_summary.join("\n")
                        );
                        accumulated_text = summary.clone();
                    } else if empty_turns_count == 1 {
                        self.context_builder.add_message(
                            chilli_context::types::Role::User,
                            "Please summarize the actions taken or provide a final response for the requested task."
                        );
                        continue;
                    } else {
                        accumulated_text = "Task processing completed successfully.".to_string();
                    }
                } else {
                    self.context_builder.add_assistant_message(&turn_text, None);
                }

                if let Some(failure_reasons) = self.check_verification()? {
                    let _ = self.journal.append(&EventRecord {
                        seq: 0,
                        session_id: "default_session".to_string(),
                        event_type: "VerificationFailed".to_string(),
                        payload_json: serde_json::json!({
                            "failures": failure_reasons,
                        })
                        .to_string(),
                        timestamp_ms: 0,
                    });

                    let attempts = self
                        .active_task
                        .as_ref()
                        .map(|t| t.verification_attempts)
                        .unwrap_or(0);
                    if attempts < 3 {
                        let feedback_msg = self.build_strategy_progression_feedback(attempts, &failure_reasons);
                        if let Some(at) = self.active_task.as_mut() {
                            at.verification_attempts += 1;
                            at.phase = TaskPhase::Recover;
                            at.verification_failures.extend(failure_reasons.clone());
                        }
                        self.context_builder
                            .add_message(chilli_context::types::Role::User, &feedback_msg);
                        continue;
                    }

                    if let Some(at) = self.active_task.as_mut() {
                        at.phase = TaskPhase::Failed;
                    }
                    self.state = ExecutionState::Failed;
                    debug!("[engine telemetry] state: {:?}", self.state);
                    return Ok(TaskResult {
                        completed: false,
                        final_output: format!(
                            "Verification failed: {}",
                            failure_reasons.join("; ")
                        ),
                    });
                }

                if let Some(at) = self.active_task.as_mut() {
                    at.phase = TaskPhase::Finished;
                }
                self.state = ExecutionState::Finished;
                debug!("[engine telemetry] state: {:?}", self.state);
                let _ = self.journal.append(&EventRecord {
                    seq: 0,
                    session_id: "default_session".to_string(),
                    event_type: "TaskCompleted".to_string(),
                    payload_json: serde_json::json!({
                        "final_output": accumulated_text,
                    })
                    .to_string(),
                    timestamp_ms: 0,
                });

                return Ok(TaskResult {
                    completed: true,
                    final_output: accumulated_text,
                });
            }
        }

        self.state = ExecutionState::Finished;
        if let Some(at) = self.active_task.as_mut() {
            at.phase = TaskPhase::Finished;
        }
        debug!("[engine telemetry] state: {:?}", self.state);
        Ok(TaskResult {
            completed: true,
            final_output: if accumulated_text.is_empty() {
                "Finished task execution.".to_string()
            } else {
                accumulated_text
            },
        })
    }

    pub async fn run_task(&mut self, task: &str) -> Result<TaskResult, String> {
        if self.mock_responses.is_empty() {
            let default_adapter = chilli_model::create_adapter_for_model(None);
            return self.run_task_with_adapter(task, default_adapter).await;
        }

        self.set_state(ExecutionState::AssemblingContext);
        self.context_builder.clear_messages();
        let sys_prompt = self.build_system_prompt();
        self.context_builder.set_system_prompt(&sys_prompt);
        let mut active_state = ActiveTaskState::new(task);
        active_state.phase = TaskPhase::Understand;
        self.context_builder
            .set_tools_declaration(Self::tools_declaration_for_phase(active_state.phase));
        self.context_builder.add_user_task(task);
        self.emit_event(EngineEvent::TurnStarted {
            session_id: "default_session".to_string(),
            prompt: task.to_string(),
        });
        let _ = self.context_builder.build_cache_aligned();
        self.active_task = Some(active_state);
        let mut zero_tool_retries = 0;

        while !self.mock_responses.is_empty() {
            if self.cancel_flag.load(Ordering::SeqCst) {
                self.state = ExecutionState::Failed;
                if let Some(at) = self.active_task.as_mut() {
                    at.phase = TaskPhase::Failed;
                }
                return Err("Execution cancelled".to_string());
            }

            self.state = ExecutionState::RequestingModel;
            let mock_resp = self.mock_responses.remove(0);

            match mock_resp {
                MockResponse::ToolCall { name, arguments } => {
                    self.state = ExecutionState::ParsingToolCalls;
                    self.state = ExecutionState::EvaluatingPolicy;

                    if let Some(at) = self.active_task.as_mut() {
                        at.executed_tools_summary.push(name.clone());
                        if at.phase == TaskPhase::Understand || at.phase == TaskPhase::Inspect {
                            at.phase = TaskPhase::Execute;
                        }
                    }

                    let op_id = format!(
                        "op_{}_{}",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_nanos(),
                        rand_counter()
                    );

                    let mut decision = if name == "exec" {
                        let cmd_str = arguments
                            .get("command")
                            .and_then(|c| c.as_str())
                            .unwrap_or("");
                        let cwd_str = arguments.get("cwd").and_then(|c| c.as_str()).unwrap_or(".");
                        let target_cwd = self.workspace_root.join(cwd_str);
                        self.policy
                            .check_command(&self.workspace_root, &target_cwd, cmd_str)
                    } else if name == "write_file" || name == "edit_file" {
                        let path_arg = arguments.get("path").and_then(|p| p.as_str()).unwrap_or("");
                        let target_path = self.workspace_root.join(path_arg);
                        if let Some(at) = self.active_task.as_mut() {
                            at.record_file_modification(path_arg);
                        }
                        self.policy.check_write(&self.workspace_root, &target_path)
                    } else {
                        let path_arg = arguments.get("path").and_then(|p| p.as_str()).unwrap_or("");
                        let target_path = self.workspace_root.join(path_arg);
                        self.policy.check_read(&self.workspace_root, &target_path)
                    };

                    if self.autonomy_mode == AutonomyMode::Manual
                        && matches!(decision, PolicyDecision::Allow)
                    {
                        decision = PolicyDecision::Ask(format!(
                            "Manual mode authorization required for tool '{}'",
                            name
                        ));
                    }

                    if let PolicyDecision::Deny(reason) | PolicyDecision::Ask(reason) = decision {
                        let _ = self.journal.append(&EventRecord {
                            seq: 0,
                            session_id: "default_session".to_string(),
                            event_type: "ToolDenied".to_string(),
                            payload_json: serde_json::json!({
                                "op_id": op_id,
                                "tool": name,
                                "error": reason,
                            })
                            .to_string(),
                            timestamp_ms: 0,
                        });
                        self.context_builder.add_tool_observation_with_id(
                            "call_mock",
                            &name,
                            &format!("Policy Denied: {}", reason),
                        );
                        continue;
                    }

                    let _ = self.journal.append(&EventRecord {
                        seq: 0,
                        session_id: "default_session".to_string(),
                        event_type: "ToolCallStarted".to_string(),
                        payload_json: serde_json::json!({
                            "op_id": op_id,
                            "tool": name,
                            "arguments": arguments,
                        })
                        .to_string(),
                        timestamp_ms: 0,
                    });

                    self.state = ExecutionState::ExecutingTools;
                    let tool_result =
                        self.execute_tool(&name, arguments.clone())
                            .unwrap_or_else(|e| chilli_tools::native_tools::ToolResult {
                                success: false,
                                output: format!("Tool execution error: {}", e),
                            });

                    let _ = self.journal.append(&EventRecord {
                        seq: 0,
                        session_id: "default_session".to_string(),
                        event_type: "ToolCallCommitted".to_string(),
                        payload_json: serde_json::json!({
                            "op_id": op_id,
                            "tool": name,
                        })
                        .to_string(),
                        timestamp_ms: 0,
                    });

                    let _ = self.journal.append(&EventRecord {
                        seq: 0,
                        session_id: "default_session".to_string(),
                        event_type: "ToolResultRecorded".to_string(),
                        payload_json: serde_json::json!({
                            "op_id": op_id,
                            "tool": name,
                            "success": tool_result.success,
                            "output": tool_result.output,
                        })
                        .to_string(),
                        timestamp_ms: 0,
                    });

                    let _ = self.journal.append(&EventRecord {
                        seq: 0,
                        session_id: "default_session".to_string(),
                        event_type: "ToolExecuted".to_string(),
                        payload_json: serde_json::json!({
                            "op_id": op_id,
                            "tool": name,
                            "arguments": arguments,
                            "output": tool_result.output,
                        })
                        .to_string(),
                        timestamp_ms: 0,
                    });

                    self.state = ExecutionState::RecordingObservations;
                    let obs_tokens = chilli_context::types::estimate_tokens(&tool_result.output);
                    if let Some(at) = self.active_task.as_mut() {
                        at.telemetry.tool_output_tokens += obs_tokens;
                    }
                    self.context_builder.add_tool_observation_with_id(
                        "call_mock",
                        &name,
                        &tool_result.output,
                    );
                }
                MockResponse::Text(text) => {
                    self.state = ExecutionState::CheckingCompletion;

                    let executed_tools_count = self
                        .active_task
                        .as_ref()
                        .map(|t| t.executed_tools_summary.len())
                        .unwrap_or(0);

                    if executed_tools_count == 0 && zero_tool_retries < 2 {
                        let is_ws = self
                            .active_task
                            .as_ref()
                            .map(|t| t.is_workspace_task())
                            .unwrap_or_else(|| crate::state::is_workspace_task(task));
                        if is_ws {
                            zero_tool_retries += 1;
                            if let Some(at) = self.active_task.as_mut() {
                                at.zero_tool_guard_triggered = true;
                            }
                            let feedback_msg = format!(
                                "Zero tools were invoked for workspace task (attempt {}/2). Please use file/exec tools to inspect or complete task.",
                                zero_tool_retries
                            );
                            self.context_builder
                                .add_message(chilli_context::types::Role::User, &feedback_msg);
                            continue;
                        }
                    }

                    if let Some(failure_reasons) = self.check_verification()? {
                        let _ = self.journal.append(&EventRecord {
                            seq: 0,
                            session_id: "default_session".to_string(),
                            event_type: "VerificationFailed".to_string(),
                            payload_json: serde_json::json!({
                                "failures": failure_reasons,
                            })
                            .to_string(),
                            timestamp_ms: 0,
                        });

                        let attempts = self
                            .active_task
                            .as_ref()
                            .map(|t| t.verification_attempts)
                            .unwrap_or(0);

                        if attempts < 3 {
                            let feedback_msg = self.build_strategy_progression_feedback(attempts, &failure_reasons);
                            if let Some(at) = self.active_task.as_mut() {
                                at.verification_attempts += 1;
                                at.consecutive_failures += 1;
                                at.phase = TaskPhase::Recover;
                                at.verification_failures.extend(failure_reasons.clone());
                            }
                            self.context_builder
                                .add_message(chilli_context::types::Role::User, &feedback_msg);
                            continue;
                        }

                        if let Some(at) = self.active_task.as_mut() {
                            at.phase = TaskPhase::Failed;
                        }
                        self.state = ExecutionState::Failed;
                        return Ok(TaskResult {
                            completed: false,
                            final_output: format!(
                                "Verification failed: {}",
                                failure_reasons.join("; ")
                            ),
                        });
                    }

                    if let Some(at) = self.active_task.as_mut() {
                        at.phase = TaskPhase::Finished;
                    }
                    self.state = ExecutionState::Finished;

                    let _ = self.journal.append(&EventRecord {
                        seq: 0,
                        session_id: "default_session".to_string(),
                        event_type: "TaskCompleted".to_string(),
                        payload_json: serde_json::json!({
                            "final_output": text,
                        })
                        .to_string(),
                        timestamp_ms: 0,
                    });

                    return Ok(TaskResult {
                        completed: true,
                        final_output: text,
                    });
                }
            }
        }

        if let Some(failure_reasons) = self.check_verification()? {
            let _ = self.journal.append(&EventRecord {
                seq: 0,
                session_id: "default_session".to_string(),
                event_type: "VerificationFailed".to_string(),
                payload_json: serde_json::json!({
                    "failures": failure_reasons,
                })
                .to_string(),
                timestamp_ms: 0,
            });
            self.state = ExecutionState::Failed;
            return Ok(TaskResult {
                completed: false,
                final_output: format!("Verification failed: {}", failure_reasons.join("; ")),
            });
        }

        self.state = ExecutionState::Finished;
        Ok(TaskResult {
            completed: true,
            final_output: "Finished queued execution.".to_string(),
        })
    }

    pub async fn run_task_with_adapter(
        &mut self,
        task: &str,
        adapter: Arc<dyn ModelAdapter>,
    ) -> Result<TaskResult, String> {
        let model_name_lower = adapter.model_name().to_lowercase();
        if self.prompt_profile == PromptProfile::Standard
            && (model_name_lower.contains("llama")
                || model_name_lower.contains("70b")
                || model_name_lower.contains("8b")
                || model_name_lower.contains("qwen")
                || model_name_lower.contains("haiku")
                || model_name_lower.contains("groq"))
        {
            self.prompt_profile = PromptProfile::SmallModel;
        }

        let active_state = crate::state::ActiveTaskState::new(task);
        let initial_phase = active_state.phase;
        self.active_task = Some(active_state);
        self.context_builder.clear_messages();

        self.capability_profile = chilli_model::capabilities::ModelCapabilityProfile::new(
            adapter.provider_name(),
            adapter.model_name(),
            8192,
        );

        let is_ws = crate::state::is_workspace_task(task);
        let codegraph_enabled = crate::ABLATION_CODEGRAPH.load(std::sync::atomic::Ordering::SeqCst);
        if codegraph_enabled && is_ws {
            if self.codegraph.is_none() {
                let _ = self.enable_codegraph();
            }
            if let Some(ref graph) = self.codegraph {
                self.context_builder.add_symbol_graph_context(graph, task);
            }
        }

        let sys_prompt = self.build_system_prompt();
        self.context_builder.set_system_prompt(&sys_prompt);
        self.context_builder
            .set_tools_declaration(Self::tools_declaration_for_phase(initial_phase));
        self.context_builder.add_user_task(task);

        self.emit_event(EngineEvent::TurnStarted {
            session_id: "default_session".to_string(),
            prompt: task.to_string(),
        });
        self.set_state(ExecutionState::AssemblingContext);

        let mut accumulated_text = String::new();
        let mut max_turns = self
            .active_task
            .as_ref()
            .and_then(|at| at.strategy.as_ref())
            .map(|s| s.max_turns())
            .unwrap_or(10);
        let mut turn = 0;
        let mut elastic_extended = false;
        let mut executed_tools_summary: Vec<String> = Vec::new();
        let mut empty_turns_count = 0;
        let mut consecutive_tool_failures = self
            .active_task
            .as_ref()
            .map(|at| at.consecutive_failures)
            .unwrap_or(0);
        let mut last_failed_call: Option<(String, serde_json::Value)> = None;
        let mut zero_tool_retries = 0usize;

        while turn < max_turns {
            turn += 1;
            if self.cancel_flag.load(Ordering::SeqCst) {
                self.set_state(ExecutionState::Failed);
                return Err("Execution cancelled".to_string());
            }

            let current_phase = self
                .active_task
                .as_ref()
                .map(|t| t.phase)
                .unwrap_or(TaskPhase::Understand);
            self.context_builder
                .set_tools_declaration(Self::tools_declaration_for_phase(current_phase));

            // Sync out-of-band checkpoint state to context builder
            if let Some(at) = &self.active_task {
                let chk_summary = format!(
                    "Phase: {:?}, Modified Files: {:?}, Verification Attempts: {}",
                    at.phase, at.modified_files, at.verification_attempts
                );
                self.context_builder.set_checkpoint_summary(&chk_summary);
            }

            // Check for elastic turn extension when reaching base budget in CheckpointedLoop strategy
            if turn == max_turns && !elastic_extended {
                if let Some(at) = &self.active_task {
                    if let Some(crate::state::ExecutionStrategy::CheckpointedLoop {
                        elastic_extension,
                        ..
                    }) = at.strategy
                    {
                        if !at.modified_files.is_empty() || at.verification_attempts > 0 {
                            elastic_extended = true;
                            max_turns += elastic_extension;
                            debug!(
                                "[engine telemetry] Elastic turn budget extended: +{} turns (new limit: {})",
                                elastic_extension, max_turns
                            );
                        }
                    }
                }
            }

            self.set_state(ExecutionState::RequestingModel);

            let openai_msgs = self.context_builder.build_openai_messages();
            let formatted = self.context_builder.build().unwrap_or_else(|_| {
                chilli_context::types::FormattedPrompt {
                    system_prompt: "You are Chilli AI agent runtime.".to_string(),
                    tools_json: serde_json::json!([]),
                    messages: vec![],
                    estimated_tokens: 0,
                }
            });

            let sys_sum = if formatted.system_prompt.chars().count() > 40 {
                format!(
                    "{}...",
                    formatted
                        .system_prompt
                        .chars()
                        .take(40)
                        .collect::<String>()
                        .replace('\n', " ")
                )
            } else {
                formatted.system_prompt.replace('\n', " ")
            };
            let mut diag_parts = vec![format!("system: {}", sys_sum)];
            for msg in &openai_msgs {
                let role = msg
                    .get("role")
                    .and_then(|r| r.as_str())
                    .unwrap_or("unknown");
                let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
                let summary = if content.chars().count() > 40 {
                    format!(
                        "{}...",
                        content
                            .chars()
                            .take(40)
                            .collect::<String>()
                            .replace('\n', " ")
                    )
                } else if msg.get("tool_calls").is_some() {
                    "[tool_calls]".to_string()
                } else {
                    content.replace('\n', " ")
                };
                diag_parts.push(format!("{}: {}", role, summary));
            }
            if std::env::var("CHILLI_DEBUG").is_ok() {
                debug!(
                    "[PRE-REQUEST DIAGNOSTIC] turn {}: [{}]",
                    turn,
                    diag_parts.join(", ")
                );
            }
            let prompt = formatted
                .messages
                .last()
                .map(|m| m.content.clone())
                .unwrap_or_else(|| task.to_string());
            let req = CompletionRequest {
                prompt,
                system_prompt: Some(formatted.system_prompt),
                tools_json: Some(formatted.tools_json),
                messages: if openai_msgs.is_empty() {
                    None
                } else {
                    Some(openai_msgs)
                },
                max_tokens: None,
                reasoning_effort: None,
            };

            let mut stream = adapter.complete(req).await?;
            let mut accumulator = crate::stream_accumulator::StreamAccumulator::new();

            while let Some(event) = stream.next().await {
                if let chilli_model::adapter::StreamEvent::TextChunk(ref text) = event {
                    self.emit_event(EngineEvent::ModelStreamChunk {
                        delta: text.clone(),
                    });
                }
                accumulator.feed(&event);
            }

            let _ = self.journal.append(&EventRecord {
                seq: 0,
                session_id: "default_session".to_string(),
                event_type: "ModelTurnCompleted".to_string(),
                payload_json: serde_json::json!({
                    "prompt_tokens": accumulator.token_usage.prompt_tokens,
                    "completion_tokens": accumulator.token_usage.completion_tokens,
                    "cached_tokens": accumulator.token_usage.cached_tokens,
                })
                .to_string(),
                timestamp_ms: 0,
            });

            let turn_text = accumulator.accumulated_text().to_string();
            accumulated_text.push_str(&turn_text);

            let tool_calls = accumulator.finish_tool_calls().unwrap_or_default();

            if !tool_calls.is_empty() {
                empty_turns_count = 0;
                self.set_state(ExecutionState::ParsingToolCalls);
                debug!("[engine telemetry] state: {:?}", self.state);

                let formatted_tool_calls: Vec<serde_json::Value> = tool_calls
                    .iter()
                    .enumerate()
                    .map(|(i, tc)| {
                        let id = if tc.id.is_empty() {
                            format!("call_{}", i)
                        } else {
                            tc.id.clone()
                        };
                        serde_json::json!({
                            "id": id,
                            "type": "function",
                            "function": {
                                "name": tc.name,
                                "arguments": tc.raw_arguments_json
                            }
                        })
                    })
                    .collect();

                self.context_builder.add_assistant_message(
                    &turn_text,
                    Some(serde_json::json!(formatted_tool_calls)),
                );

                for (i, tool_call) in tool_calls.into_iter().enumerate() {
                    let name = tool_call.name;
                    let args_val = tool_call.arguments;
                    let call_id = if tool_call.id.is_empty() {
                        format!("call_{}", i)
                    } else {
                        tool_call.id
                    };

                    self.set_state(ExecutionState::EvaluatingPolicy);
                    debug!("[engine telemetry] state: {:?}", self.state);
                    let op_id = format!(
                        "op_{}_{}",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_nanos(),
                        rand_counter()
                    );

                    let mut decision = if name == "exec" {
                        let cmd_str = args_val
                            .get("command")
                            .and_then(|c| c.as_str())
                            .unwrap_or("");
                        let cwd_str = args_val.get("cwd").and_then(|c| c.as_str()).unwrap_or(".");
                        let target_cwd = self.workspace_root.join(cwd_str);
                        self.policy
                            .check_command(&self.workspace_root, &target_cwd, cmd_str)
                    } else if name == "write_file" || name == "edit_file" {
                        let path_arg = args_val.get("path").and_then(|p| p.as_str()).unwrap_or("");
                        let target_path = self.workspace_root.join(path_arg);
                        self.policy.check_write(&self.workspace_root, &target_path)
                    } else {
                        let path_arg = args_val.get("path").and_then(|p| p.as_str()).unwrap_or("");
                        let target_path = self.workspace_root.join(path_arg);
                        self.policy.check_read(&self.workspace_root, &target_path)
                    };

                    if self.autonomy_mode == AutonomyMode::Manual
                        && matches!(decision, PolicyDecision::Allow)
                    {
                        decision = PolicyDecision::Ask(format!(
                            "Manual mode authorization required for tool '{}'",
                            name
                        ));
                    }

                    if let PolicyDecision::Deny(reason) | PolicyDecision::Ask(reason) = decision {
                        self.emit_event(EngineEvent::ToolCallDenied {
                            tool_name: name.clone(),
                            reason: reason.clone(),
                        });
                        let _ = self.journal.append(&EventRecord {
                            seq: 0,
                            session_id: "default_session".to_string(),
                            event_type: "ToolDenied".to_string(),
                            payload_json: serde_json::json!({
                                "op_id": op_id,
                                "tool": name,
                                "error": reason,
                            })
                            .to_string(),
                            timestamp_ms: 0,
                        });
                        let err_msg = format!("Policy Denied: {}", reason);
                        executed_tools_summary.push(format!("Tool {} denied: {}", name, reason));
                        self.context_builder
                            .add_tool_observation_with_id(&call_id, &name, &err_msg);
                        continue;
                    }

                    if self.is_tool_call_locked(&name, &args_val) {
                        let loop_act = self.record_tool_failure_for_loop_detector(
                            &name,
                            &args_val,
                            "Tool call signature locked",
                        );
                        if loop_act.is_terminate() {
                            let err_msg = format!("HARD INTERVENTION: Tool call signature for '{}' is locked due to duplicate failure loop (N>=4). Task terminated.", name);
                            self.context_builder
                                .add_tool_observation_with_id(&call_id, &name, &err_msg);
                            self.state = ExecutionState::Failed;
                            if let Some(at) = self.active_task.as_mut() {
                                at.phase = crate::state::TaskPhase::Failed;
                            }
                            return Ok(TaskResult {
                                completed: false,
                                final_output: format!(
                                    "Failure budget exceeded: duplicate failure loop on tool '{}'.",
                                    name
                                ),
                            });
                        }

                        let err_msg = format!("HARD INTERVENTION: Tool call signature for '{}' is locked due to duplicate failure loop (N>=3). You MUST alter parameters, try a different tool, or perform inspection.", name);
                        executed_tools_summary
                            .push(format!("Tool {} locked: duplicate loop", name));
                        self.context_builder
                            .add_tool_observation_with_id(&call_id, &name, &err_msg);
                        continue;
                    }

                    self.emit_event(EngineEvent::ToolCallStarted {
                        tool_name: name.clone(),
                        args_json: args_val.to_string(),
                    });
                    let _ = self.journal.append(&EventRecord {
                        seq: 0,
                        session_id: "default_session".to_string(),
                        event_type: "ToolCallStarted".to_string(),
                        payload_json: serde_json::json!({
                            "op_id": op_id,
                            "tool": name,
                            "arguments": args_val,
                        })
                        .to_string(),
                        timestamp_ms: 0,
                    });

                    self.set_state(ExecutionState::ExecutingTools);
                    debug!("[engine telemetry] state: {:?}", self.state);
                    let start_t = std::time::Instant::now();
                    let tool_result =
                        self.execute_tool(&name, args_val.clone())
                            .unwrap_or_else(|e| chilli_tools::native_tools::ToolResult {
                                success: false,
                                output: format!("Tool execution error: {}", e),
                            });
                    let dur = start_t.elapsed().as_millis() as u64;
                    self.emit_event(EngineEvent::ToolExecutionCompleted {
                        tool_name: name.clone(),
                        result_summary: tool_result.output.clone(),
                        duration_ms: dur,
                    });

                    let _ = self.journal.append(&EventRecord {
                        seq: 0,
                        session_id: "default_session".to_string(),
                        event_type: "ToolCallCommitted".to_string(),
                        payload_json: serde_json::json!({
                            "op_id": op_id,
                            "tool": name,
                        })
                        .to_string(),
                        timestamp_ms: 0,
                    });

                    let _ = self.journal.append(&EventRecord {
                        seq: 0,
                        session_id: "default_session".to_string(),
                        event_type: "ToolResultRecorded".to_string(),
                        payload_json: serde_json::json!({
                            "op_id": op_id,
                            "tool": name,
                            "success": tool_result.success,
                            "output": tool_result.output,
                        })
                        .to_string(),
                        timestamp_ms: 0,
                    });

                    let _ = self.journal.append(&EventRecord {
                        seq: 0,
                        session_id: "default_session".to_string(),
                        event_type: "ToolExecuted".to_string(),
                        payload_json: serde_json::json!({
                            "op_id": op_id,
                            "tool": name,
                            "arguments": args_val,
                            "output": tool_result.output,
                        })
                        .to_string(),
                        timestamp_ms: 0,
                    });

                    self.set_state(ExecutionState::RecordingObservations);
                    debug!("[engine telemetry] state: {:?}", self.state);

                    let is_dup_failure = if !tool_result.success {
                        if let Some((ref last_name, ref last_args)) = last_failed_call {
                            last_name == &name && last_args == &args_val
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    if tool_result.success {
                        self.loop_detector.record_success(&name, &args_val);
                        self.capability_profile.record_observation(
                            chilli_model::capabilities::ObservationSample {
                                tool_call_accuracy: 1.0,
                                instruction_following: 1.0,
                                verification_success: 1.0,
                                recovery_success: 1.0,
                            },
                        );
                        consecutive_tool_failures = 0;
                        last_failed_call = None;
                        executed_tools_summary.push(format!(
                            "Executed {} successfully: {}",
                            name,
                            tool_result.output.trim()
                        ));
                        if let Some(at) = self.active_task.as_mut() {
                            at.executed_tools_summary.push(format!("{} (ok)", name));
                            at.consecutive_failures = 0;
                            if let Some(path_str) = args_val.get("path").and_then(|p| p.as_str()) {
                                at.record_file_visit(path_str);
                            }
                            if name == "write_file" || name == "edit_file" {
                                if let Some(path_str) =
                                    args_val.get("path").and_then(|p| p.as_str())
                                {
                                    at.record_file_modification(path_str);
                                }
                                at.phase = TaskPhase::Execute;
                            } else if at.phase == TaskPhase::Understand {
                                at.phase = TaskPhase::Inspect;
                            }
                        }
                        self.context_builder.add_tool_observation_with_id(
                            &call_id,
                            &name,
                            &tool_result.output,
                        );
                    } else {
                        self.capability_profile.record_observation(
                            chilli_model::capabilities::ObservationSample {
                                tool_call_accuracy: 0.0,
                                instruction_following: 0.5,
                                verification_success: 0.0,
                                recovery_success: 0.0,
                            },
                        );
                        consecutive_tool_failures += 1;
                        last_failed_call = Some((name.clone(), args_val.clone()));

                        let loop_act = self.record_tool_failure_for_loop_detector(
                            &name,
                            &args_val,
                            &tool_result.output,
                        );
                        if loop_act.is_lock() {
                            self.prompt_profile = PromptProfile::Frontier;
                        }

                        let mut obs_output = tool_result.output.clone();
                        if loop_act.is_lock() {
                            self.prompt_profile = PromptProfile::Frontier;
                            obs_output.push_str(
                                "\n[HARD INTERVENTION: Duplicate failure threshold reached (N=3). \
                                 You MUST STOP repeating similar edits. Pause, perform a fresh diagnosis from scratch, \
                                 inspect actual file contents with read_file, and verify your assumptions before modifying files again.]"
                            );
                        } else if is_dup_failure {
                            obs_output.push_str(
                                "\n[SYSTEM NOTICE: Identical tool call failed again. Do NOT retry the exact same parameters without modifying your approach.]"
                            );
                        }

                        if loop_act.is_terminate() || consecutive_tool_failures >= 5 {
                            obs_output.push_str("\n[SYSTEM NOTICE: Task terminated due to duplicate failure loop (N=4) or failure budget.]");
                            self.context_builder.add_tool_observation_with_id(
                                &call_id,
                                &name,
                                &obs_output,
                            );
                            self.state = ExecutionState::Failed;
                            if let Some(at) = self.active_task.as_mut() {
                                at.phase = crate::state::TaskPhase::Failed;
                            }
                            return Ok(TaskResult {
                                completed: false,
                                final_output: format!("Task terminated: duplicate failure loop or budget exceeded on tool '{}'.", name),
                            });
                        }

                        executed_tools_summary.push(format!(
                            "Tool {} failed: {}",
                            name,
                            tool_result.output.trim()
                        ));
                        if let Some(at) = self.active_task.as_mut() {
                            at.executed_tools_summary.push(format!("{} (err)", name));
                            at.consecutive_failures = consecutive_tool_failures;
                        }
                        self.context_builder.add_tool_observation_with_id(
                            &call_id,
                            &name,
                            &obs_output,
                        );
                    }
                }
            } else {
                if executed_tools_summary.is_empty() && zero_tool_retries < 2 {
                    let is_ws = self
                        .active_task
                        .as_ref()
                        .map(|t| t.is_workspace_task())
                        .unwrap_or_else(|| crate::state::is_workspace_task(task));
                    if is_ws {
                        zero_tool_retries += 1;
                        if let Some(at) = self.active_task.as_mut() {
                            at.zero_tool_guard_triggered = true;
                        }
                        if !turn_text.trim().is_empty() {
                            self.context_builder.add_assistant_message(&turn_text, None);
                        }
                        self.context_builder.add_message(
                            chilli_context::types::Role::User,
                            "Task requires modifying repository files or executing commands, but zero tools were called. Inspect the workspace or make the required edit using tools before completing."
                        );
                        continue;
                    }
                }

                if turn_text.trim().is_empty() {
                    empty_turns_count += 1;
                    if !executed_tools_summary.is_empty() {
                        let summary = format!(
                            "Completed task actions:\n{}",
                            executed_tools_summary.join("\n")
                        );
                        accumulated_text = summary.clone();
                    } else if empty_turns_count == 1 {
                        self.context_builder.add_message(
                            chilli_context::types::Role::User,
                            "Please summarize the actions taken or provide a final response for the requested task."
                        );
                        continue;
                    } else {
                        accumulated_text = "Task processing completed successfully.".to_string();
                    }
                } else {
                    self.context_builder.add_assistant_message(&turn_text, None);
                }

                if let Some(failure_reasons) = self.check_verification()? {
                    let _ = self.journal.append(&EventRecord {
                        seq: 0,
                        session_id: "default_session".to_string(),
                        event_type: "VerificationFailed".to_string(),
                        payload_json: serde_json::json!({
                            "failures": failure_reasons,
                        })
                        .to_string(),
                        timestamp_ms: 0,
                    });

                    let attempts = self
                        .active_task
                        .as_ref()
                        .map(|t| t.verification_attempts)
                        .unwrap_or(0);
                    if attempts < 3 {
                        let feedback_msg = self.build_strategy_progression_feedback(attempts, &failure_reasons);
                        if let Some(at) = self.active_task.as_mut() {
                            at.verification_attempts += 1;
                            at.phase = TaskPhase::Recover;
                            at.verification_failures.extend(failure_reasons.clone());
                        }
                        self.context_builder
                            .add_message(chilli_context::types::Role::User, &feedback_msg);
                        continue;
                    }

                    if let Some(at) = self.active_task.as_mut() {
                        at.phase = TaskPhase::Failed;
                    }
                    self.state = ExecutionState::Failed;
                    debug!("[engine telemetry] state: {:?}", self.state);
                    return Ok(TaskResult {
                        completed: false,
                        final_output: format!(
                            "Verification failed: {}",
                            failure_reasons.join("; ")
                        ),
                    });
                }

                if let Some(at) = self.active_task.as_mut() {
                    at.phase = TaskPhase::Finished;
                }
                self.state = ExecutionState::Finished;
                debug!("[engine telemetry] state: {:?}", self.state);
                let _ = self.journal.append(&EventRecord {
                    seq: 0,
                    session_id: "default_session".to_string(),
                    event_type: "TaskCompleted".to_string(),
                    payload_json: serde_json::json!({
                        "final_output": accumulated_text,
                    })
                    .to_string(),
                    timestamp_ms: 0,
                });

                return Ok(TaskResult {
                    completed: true,
                    final_output: accumulated_text,
                });
            }
        }

        self.state = ExecutionState::Finished;
        debug!("[engine telemetry] state: {:?}", self.state);
        Ok(TaskResult {
            completed: true,
            final_output: if accumulated_text.is_empty() {
                "Finished task execution.".to_string()
            } else {
                accumulated_text
            },
        })
    }

    pub fn execute_tool(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<chilli_tools::native_tools::ToolResult, String> {
        debug!("[TOOL EXECUTE DEBUG] tool={}, args={}", name, args);
        let path_opt = args
            .get("path")
            .and_then(|p| p.as_str())
            .map(|s| s.to_string());
        let mut res = match name {
            "write_file" => self.write_tool.execute(args),
            "read_file" => self.read_tool.execute(args),
            "edit_file" => self.edit_tool.execute(args),
            "grep" => self.grep_tool.execute(args),
            "list_directory" => self.list_dir_tool.execute(args),
            "exec" => self.exec_tool.execute(args),
            "move_file" => self.move_tool.execute(args),
            "copy_file" => self.copy_tool.execute(args),
            "create_directory" => self.create_dir_tool.execute(args),
            "remove_file" => self.remove_tool.execute(args),
            "find_symbol" => self.find_symbol_tool.execute(args),
            "find_references" => self.find_references_tool.execute(args),
            "find_test_for_failure" => self.find_test_for_failure_tool.execute(args),
            _ => Err(format!("Unknown tool: {}", name)),
        };
        if let Err(ref e) = res {
            debug!("[TOOL EXECUTE ERROR] tool={}, err={}", name, e);
        } else if let Ok(ref mut tool_res) = res {
            if (name == "write_file" || name == "edit_file") && tool_res.success {
                if let Some(path_str) = path_opt {
                    let full_path = self.workspace_root.join(&path_str);
                    if let Ok(content) = std::fs::read_to_string(&full_path) {
                        if let Ok(graph) = chilli_repo_intel::CodeGraph::open_in_memory() {
                            let diagnostics = graph.check_syntax(&full_path, &content);
                            if !diagnostics.is_empty() {
                                let diag_msgs: Vec<String> = diagnostics
                                    .iter()
                                    .take(3)
                                    .map(|d| {
                                        format!(
                                            "line {}, col {}: {}",
                                            d.line_number, d.column, d.message
                                        )
                                    })
                                    .collect();
                                tool_res.output.push_str(&format!(
                                    "\n[SYNTAX DIAGNOSTIC WARNING: AST syntax error(s) detected in '{}':\n  {}]",
                                    path_str,
                                    diag_msgs.join("\n  ")
                                ));
                            }
                        }
                    }
                }

                if self.workspace_root.join("Cargo.toml").exists() {
                    let lsp_diags =
                        chilli_repo_intel::LspDiagnosticClient::collect_rust_diagnostics(
                            &self.workspace_root,
                        );
                    if !lsp_diags.is_empty() {
                        let formatted_lsp =
                            chilli_repo_intel::LspDiagnosticClient::format_diagnostics_for_prompt(
                                &lsp_diags,
                            );
                        tool_res.output.push_str(&formatted_lsp);
                    }
                }
            }
        }
        res
    }

    pub fn build_strategy_progression_feedback(
        &self,
        attempts: usize,
        failure_reasons: &[String],
    ) -> String {
        let combined_tb = failure_reasons.join("\n");
        let step_num = attempts + 1;
        match step_num {
            1 => {
                let mut msg = format!(
                    "[STRATEGY STEP 1/4: LOCAL REPAIR]\nVerification check failed:\n{}\n",
                    combined_tb
                );
                if let Some((rel_test, test_slice)) =
                    chilli_verification::extract_failing_test_assertion(&self.workspace_root, &combined_tb)
                {
                    msg.push_str(&format!(
                        "\n--- FAILING TEST ASSERTION SOURCE ({}) ---\n{}\n",
                        rel_test, test_slice
                    ));
                }
                msg.push_str("\nPlease inspect target file and attempt a localized fix using edit_file/write_file.");
                msg
            }
            2 => {
                let mut msg = format!(
                    "[STRATEGY STEP 2/4: DIAGNOSTIC-GUIDED REPAIR]\nVerification check failed on second attempt:\n{}\n",
                    combined_tb
                );
                if let Some((rel_test, test_slice)) =
                    chilli_verification::extract_failing_test_assertion(&self.workspace_root, &combined_tb)
                {
                    msg.push_str(&format!(
                        "\n--- FAILING TEST ASSERTION SOURCE ({}) ---\n{}\n",
                        rel_test, test_slice
                    ));
                }
                msg.push_str("\nPerform diagnostic-guided analysis. Use find_test_for_failure or read_file with offset/limit to inspect failing test assertions before making edits.");
                msg
            }
            3 => {
                let mut msg = format!(
                    "[STRATEGY STEP 3/4: CONTEXT EXPANSION + RE-PLAN]\nVerification check failed on third attempt:\n{}\n",
                    combined_tb
                );
                if let Some(ref cg) = self.codegraph {
                    let main_files: Vec<String> = self
                        .active_task
                        .as_ref()
                        .map(|t| t.modified_files.clone())
                        .unwrap_or_default();
                    let src_file = main_files.first().map(|s| s.as_str()).unwrap_or("");
                    if !src_file.is_empty() {
                        let deps = cg.resolve_cross_module_dependencies(&self.workspace_root, src_file, Some(&combined_tb));
                        msg.push_str(&format!(
                            "\n--- EXPANDED CROSS-MODULE DEPENDENCIES ---\nRelated files: {}\n",
                            deps.join(", ")
                        ));
                    }
                }
                msg.push_str("\nExpand context beyond single file. Use find_symbol, find_references, and read_file to investigate cross-module contracts and re-plan your fix.");
                msg
            }
            _ => {
                format!(
                    "[STRATEGY STEP 4/4: STOP / ESCALATE]\nStrategy progression budget (4 attempts) exhausted. Verification failed:\n{}",
                    combined_tb
                )
            }
        }
    }
}
