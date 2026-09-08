use crate::compactor::Compactor;
use crate::truncator::ObservationTruncator;
use crate::types::{
    estimate_tokens, CacheAlignedPrompt, ContextEngineTelemetry, FormattedPrompt, MessageItem, Role,
};
use chilli_memory::{MemoryEntry, PersistentMemoryEngine, TaskCheckpoint, TaskSanitizer};
use chilli_repo_intel::repo_intel::CodeGraph;
use chilli_repo_intel::store::SymbolRecord;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

pub fn format_checkpoint_resume_prompt(checkpoint: &TaskCheckpoint, notices: &[String]) -> String {
    let mut prompt = String::new();
    prompt.push_str("## RESUMED EXECUTION STATE\n");
    prompt.push_str("You are resuming execution from a prior validated task checkpoint.\n\n");
    prompt.push_str(&format!(
        "**Goal:** {}\n",
        TaskSanitizer::sanitize_text(&checkpoint.goal)
    ));
    prompt.push_str(&format!("**Phase:** {}\n", checkpoint.phase));
    if let Some(ref strat) = checkpoint.execution_strategy {
        prompt.push_str(&format!("**Execution Strategy:** {}\n", strat));
    }

    const MAX_ITEMS: usize = 20;

    if !checkpoint.completed_subtasks.is_empty() {
        prompt.push_str("\n### Completed Milestones / Subtasks:\n");
        let count = checkpoint.completed_subtasks.len();
        for subtask in checkpoint.completed_subtasks.iter().take(MAX_ITEMS) {
            prompt.push_str(&format!(
                "- [x] {}\n",
                TaskSanitizer::sanitize_text(subtask)
            ));
        }
        if count > MAX_ITEMS {
            prompt.push_str(&format!("- ... and {} more items\n", count - MAX_ITEMS));
        }
    }

    if !checkpoint.pending_subtasks.is_empty() {
        prompt.push_str("\n### Remaining Pending Subtasks:\n");
        let count = checkpoint.pending_subtasks.len();
        for subtask in checkpoint.pending_subtasks.iter().take(MAX_ITEMS) {
            prompt.push_str(&format!(
                "- [ ] {}\n",
                TaskSanitizer::sanitize_text(subtask)
            ));
        }
        if count > MAX_ITEMS {
            prompt.push_str(&format!("- ... and {} more items\n", count - MAX_ITEMS));
        }
    }

    let safe_modified: Vec<&String> = checkpoint
        .modified_files
        .iter()
        .filter(|f| !is_credential_file(f) && !TaskSanitizer::is_sensitive_file(f))
        .collect();

    if !safe_modified.is_empty() {
        prompt.push_str("\n### Files Previously Modified by Chilli:\n");
        let count = safe_modified.len();
        for file in safe_modified.iter().take(MAX_ITEMS) {
            prompt.push_str(&format!("- {}\n", file));
        }
        if count > MAX_ITEMS {
            prompt.push_str(&format!("- ... and {} more items\n", count - MAX_ITEMS));
        }
    }

    let safe_visited: Vec<&String> = checkpoint
        .visited_files
        .iter()
        .filter(|f| !is_credential_file(f) && !TaskSanitizer::is_sensitive_file(f))
        .collect();

    if !safe_visited.is_empty() {
        prompt.push_str("\n### Files Visited / Inspected:\n");
        let count = safe_visited.len();
        for file in safe_visited.iter().take(MAX_ITEMS) {
            prompt.push_str(&format!("- {}\n", file));
        }
        if count > MAX_ITEMS {
            prompt.push_str(&format!("- ... and {} more items\n", count - MAX_ITEMS));
        }
    }

    if !checkpoint.executed_tools_summary.is_empty() {
        prompt.push_str("\n### Prior Executed Tools Summary:\n");
        let count = checkpoint.executed_tools_summary.len();
        for tool_summary in checkpoint.executed_tools_summary.iter().take(MAX_ITEMS) {
            prompt.push_str(&format!(
                "- {}\n",
                TaskSanitizer::sanitize_text(tool_summary)
            ));
        }
        if count > MAX_ITEMS {
            prompt.push_str(&format!("- ... and {} more items\n", count - MAX_ITEMS));
        }
    }

    if !checkpoint.verification_failures.is_empty() {
        prompt.push_str("\n### Prior Verification Failures:\n");
        let count = checkpoint.verification_failures.len();
        for failure in checkpoint.verification_failures.iter().take(MAX_ITEMS) {
            prompt.push_str(&format!("- {}\n", TaskSanitizer::sanitize_text(failure)));
        }
        if count > MAX_ITEMS {
            prompt.push_str(&format!("- ... and {} more items\n", count - MAX_ITEMS));
        }
    }

    if !notices.is_empty() {
        prompt.push_str("\n### Workspace & Recovery Notices:\n");
        for notice in notices {
            prompt.push_str(&format!(
                "- NOTICE: {}\n",
                TaskSanitizer::sanitize_text(notice)
            ));
        }
    }

    prompt.push_str("\n### Execution Instructions:\n");
    prompt.push_str("1. Inspect the workspace if needed to confirm current file state.\n");
    prompt.push_str(
        "2. Do NOT re-execute already completed tools or duplicate completed subtasks.\n",
    );
    prompt.push_str("3. Continue execution from the pending subtasks toward verifying and completing the goal.\n");

    prompt
}

pub fn is_credential_file(path: &str) -> bool {
    let p = path.to_lowercase().replace('\\', "/");
    let filename = std::path::Path::new(&p)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&p);

    filename == ".env" || filename.starts_with(".env.") || filename.starts_with(".env_")
}

pub fn stabilize_tool_schemas(tools: &mut serde_json::Value) {
    if let serde_json::Value::Array(ref mut arr) = tools {
        arr.sort_by(|a, b| {
            let name_a = extract_tool_name(a);
            let name_b = extract_tool_name(b);
            name_a.cmp(&name_b)
        });
    }
}

fn extract_tool_name(v: &serde_json::Value) -> String {
    if let Some(n) = v.get("name").and_then(|s| s.as_str()) {
        n.to_string()
    } else if let Some(n) = v
        .get("function")
        .and_then(|f| f.get("name"))
        .and_then(|s| s.as_str())
    {
        n.to_string()
    } else {
        v.to_string()
    }
}

#[derive(Debug, Clone, Default)]
pub struct CodeGraphQueryCache {
    symbol_cache: HashMap<String, Vec<SymbolRecord>>,
    file_symbol_cache: HashMap<String, Vec<SymbolRecord>>,
}

impl CodeGraphQueryCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn invalidate_file(&mut self, path: &str) {
        let clean_path = path.replace('\\', "/");
        self.file_symbol_cache.remove(&clean_path);
        self.symbol_cache.retain(|_, symbols| {
            !symbols
                .iter()
                .any(|s| s.file_path.replace('\\', "/") == clean_path)
        });
    }

    pub fn clear(&mut self) {
        self.symbol_cache.clear();
        self.file_symbol_cache.clear();
    }
}

#[derive(Debug, Clone)]
pub struct ContextBuilder {
    max_context_tokens: usize,
    static_system_prompt: String,
    dynamic_system_prompt: String,
    system_prompt: String,
    tools_json: serde_json::Value,
    ast_symbol_prompt: String,
    memory_prompt: String,
    checkpoint_summary: Option<String>,
    messages: Vec<MessageItem>,
    pub query_cache: CodeGraphQueryCache,
}

#[derive(Debug, Clone)]
struct PriorityItem {
    #[allow(dead_code)]
    rank: u8,
    #[allow(dead_code)]
    key: String,
    formatted: String,
    tokens: usize,
    is_memory: bool,
}

impl ContextBuilder {
    pub fn new(max_context_tokens: usize) -> Self {
        Self {
            max_context_tokens,
            static_system_prompt: String::new(),
            dynamic_system_prompt: String::new(),
            system_prompt: String::new(),
            tools_json: serde_json::json!([]),
            ast_symbol_prompt: String::new(),
            memory_prompt: String::new(),
            checkpoint_summary: None,
            messages: Vec::new(),
            query_cache: CodeGraphQueryCache::new(),
        }
    }

    pub fn set_checkpoint_summary(&mut self, summary: &str) {
        self.checkpoint_summary = Some(summary.to_string());
    }

    pub fn set_static_system_prompt(&mut self, prompt: &str) {
        self.static_system_prompt = prompt.to_string();
        self.update_combined_system_prompt();
    }

    pub fn set_dynamic_system_prompt(&mut self, prompt: &str) {
        self.dynamic_system_prompt = prompt.to_string();
        self.update_combined_system_prompt();
    }

    pub fn set_system_prompt(&mut self, prompt: &str) {
        self.static_system_prompt = prompt.to_string();
        self.dynamic_system_prompt.clear();
        self.system_prompt = prompt.to_string();
    }

    fn update_combined_system_prompt(&mut self) {
        if self.dynamic_system_prompt.is_empty() {
            self.system_prompt = self.static_system_prompt.clone();
        } else if self.static_system_prompt.is_empty() {
            self.system_prompt = self.dynamic_system_prompt.clone();
        } else {
            self.system_prompt = format!(
                "{}\n\n{}",
                self.static_system_prompt, self.dynamic_system_prompt
            );
        }
    }

    pub fn set_tools_declaration(&mut self, mut tools: serde_json::Value) {
        stabilize_tool_schemas(&mut tools);
        self.tools_json = tools;
    }

    pub fn clear_messages(&mut self) {
        self.messages.clear();
    }

    pub fn add_user_task(&mut self, task: &str) {
        self.messages.push(MessageItem {
            role: Role::User,
            content: task.to_string(),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        });
    }

    pub fn add_resumed_task_context(&mut self, checkpoint: &TaskCheckpoint, notices: &[String]) {
        self.clear_messages();
        let prompt = format_checkpoint_resume_prompt(checkpoint, notices);
        self.set_checkpoint_summary(&prompt);
        self.add_user_task(&prompt);
    }

    pub fn reconstruct_from_checkpoint(
        &mut self,
        checkpoint: &TaskCheckpoint,
        notices: &[String],
        graph: Option<&CodeGraph>,
        memory_engine: Option<&PersistentMemoryEngine>,
    ) -> ContextEngineTelemetry {
        self.add_resumed_task_context(checkpoint, notices);

        let clean_goal = TaskSanitizer::sanitize_text(&checkpoint.goal);
        let safe_modified: Vec<String> = checkpoint
            .modified_files
            .iter()
            .filter(|f| !is_credential_file(f) && !TaskSanitizer::is_sensitive_file(f))
            .cloned()
            .collect();
        let safe_visited: Vec<String> = checkpoint
            .visited_files
            .iter()
            .filter(|f| !is_credential_file(f) && !TaskSanitizer::is_sensitive_file(f))
            .cloned()
            .collect();

        if let Some(cg) = graph {
            self.build_dynamic_tier3_context(
                cg,
                &clean_goal,
                &safe_modified,
                &safe_visited,
                memory_engine,
            )
        } else {
            ContextEngineTelemetry::default()
        }
    }

    pub fn add_assistant_message(&mut self, content: &str, tool_calls: Option<serde_json::Value>) {
        self.messages.push(MessageItem {
            role: Role::Assistant,
            content: content.to_string(),
            name: None,
            tool_calls,
            tool_call_id: None,
        });
    }

    pub fn add_observation(&mut self, name: &str, output: &str) {
        self.add_tool_observation_with_id("call_default", name, output);
    }

    pub fn add_tool_observation_with_id(&mut self, tool_call_id: &str, name: &str, output: &str) {
        let max_obs_tokens = 1000;
        let truncated = ObservationTruncator::truncate_tool_output(name, output, max_obs_tokens);
        self.messages.push(MessageItem {
            role: Role::Tool,
            content: truncated,
            name: Some(name.to_string()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.to_string()),
        });
    }

    pub fn add_message(&mut self, role: Role, content: &str) {
        self.messages.push(MessageItem {
            role,
            content: content.to_string(),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        });
    }

    pub fn build_openai_messages(&self) -> Vec<serde_json::Value> {
        let formatted = match self.build() {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };

        let mut json_msgs = Vec::new();
        for msg in &formatted.messages {
            match msg.role {
                Role::System => {
                    json_msgs.push(serde_json::json!({
                        "role": "system",
                        "content": msg.content
                    }));
                }
                Role::User => {
                    json_msgs.push(serde_json::json!({
                        "role": "user",
                        "content": msg.content
                    }));
                }
                Role::Assistant => {
                    let mut obj = serde_json::json!({
                        "role": "assistant"
                    });
                    if !msg.content.is_empty() {
                        obj["content"] = serde_json::json!(msg.content);
                    } else if msg.tool_calls.is_some() {
                        obj["content"] = serde_json::Value::Null;
                    } else {
                        obj["content"] = serde_json::json!("");
                    }
                    if let Some(tcs) = &msg.tool_calls {
                        obj["tool_calls"] = tcs.clone();
                    }
                    json_msgs.push(obj);
                }
                Role::Tool => {
                    let call_id = msg.tool_call_id.as_deref().unwrap_or("call_default");
                    let name = msg.name.as_deref().unwrap_or("tool");
                    json_msgs.push(serde_json::json!({
                        "role": "tool",
                        "tool_call_id": call_id,
                        "name": name,
                        "content": msg.content
                    }));
                }
            }
        }
        json_msgs
    }

    pub fn add_symbol_graph_context(&mut self, graph: &CodeGraph, task: &str) {
        self.add_targeted_codegraph_context(graph, task, &[]);
    }

    pub fn add_targeted_codegraph_context(
        &mut self,
        graph: &CodeGraph,
        task: &str,
        known_files: &[String],
    ) {
        self.build_dynamic_tier3_context(graph, task, &[], known_files, None);
    }

    pub fn add_memory_entries(&mut self, memories: &[MemoryEntry]) {
        if memories.is_empty() {
            self.memory_prompt.clear();
            return;
        }

        let mut lines = Vec::new();
        for m in memories {
            let scope = if m.is_global { "global" } else { "project" };
            lines.push(format!(
                "- [{}:{}] {}: {}",
                scope,
                m.category.as_str(),
                m.key,
                m.content
            ));
        }

        self.memory_prompt = format!("## Persistent Memory Context\n{}", lines.join("\n"));
    }

    pub fn add_memory_context(
        &mut self,
        engine: &PersistentMemoryEngine,
        query: Option<&str>,
    ) -> Result<usize, String> {
        let memories = engine
            .recall_combined(None, query)
            .map_err(|e| format!("Memory recall error: {}", e))?;
        let count = memories.len();
        self.add_memory_entries(&memories);
        Ok(count)
    }

    pub fn inject_personalization_memories(&mut self, memories: &[String], max_tokens: usize) {
        let mut header = String::from("## Personalization & User Preferences\n");
        let mut token_count = crate::types::estimate_tokens(&header);

        for (idx, mem) in memories.iter().take(5).enumerate() {
            let entry = format!("{}. {}\n", idx + 1, mem);
            let entry_tokens = crate::types::estimate_tokens(&entry);
            if token_count + entry_tokens > max_tokens {
                break;
            }
            header.push_str(&entry);
            token_count += entry_tokens;
        }

        self.memory_prompt = header;
    }

    /// Dynamic Tier-3 context scope expansion and priority ranking under 1,500 token hard cap.
    pub fn build_dynamic_tier3_context(
        &mut self,
        graph: &CodeGraph,
        task: &str,
        modified_files: &[String],
        visited_files: &[String],
        memory_engine: Option<&PersistentMemoryEngine>,
    ) -> ContextEngineTelemetry {
        let start_time = Instant::now();
        let mut telemetry = ContextEngineTelemetry::default();
        let mut candidate_items: Vec<PriorityItem> = Vec::new();
        let mut seen_keys = HashSet::new();

        // 1. Extract words from task and recent user/assistant messages for symbol reference matching
        let mut query_words: Vec<String> = task
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|w| w.len() > 2)
            .map(|s| s.to_string())
            .collect();

        for file in visited_files.iter().chain(modified_files.iter()) {
            if is_credential_file(file) {
                continue;
            }
            if let Some(file_name) = std::path::Path::new(file)
                .file_stem()
                .and_then(|s| s.to_str())
            {
                if file_name.len() > 2 && !query_words.iter().any(|w| w == file_name) {
                    query_words.push(file_name.to_string());
                }
            }
        }

        // Rank 1: Active modified symbols (in files modified during task)
        for mod_file in modified_files {
            if is_credential_file(mod_file) {
                continue;
            }
            let clean_path = mod_file.replace('\\', "/");
            telemetry.codegraph_queries_count += 1;

            let file_symbols =
                if let Some(cached) = self.query_cache.file_symbol_cache.get(&clean_path) {
                    telemetry.cache_hits += 1;
                    cached.clone()
                } else {
                    telemetry.cache_misses += 1;
                    let fetched = graph.get_symbols_by_file(&clean_path).unwrap_or_default();
                    self.query_cache
                        .file_symbol_cache
                        .insert(clean_path.clone(), fetched.clone());
                    fetched
                };

            for sym in file_symbols {
                let key = format!("rank1:{}:{}", sym.file_path, sym.name);
                if seen_keys.insert(key.clone()) {
                    let formatted = format!(
                        "- [Active Modified] Symbol: {} ({}) in {}:{}-{}",
                        sym.name, sym.kind, sym.file_path, sym.start_line, sym.end_line
                    );
                    let tokens = estimate_tokens(&formatted);
                    candidate_items.push(PriorityItem {
                        rank: 1,
                        key,
                        formatted,
                        tokens,
                        is_memory: false,
                    });
                }
            }
        }

        // Rank 2: Directly referenced symbols (named in prompt, recent messages, or visited files)
        for word in &query_words {
            telemetry.codegraph_queries_count += 1;
            let symbols = if let Some(cached) = self.query_cache.symbol_cache.get(word) {
                telemetry.cache_hits += 1;
                cached.clone()
            } else {
                telemetry.cache_misses += 1;
                let fetched = graph.lookup_symbol(word).unwrap_or_default();
                self.query_cache
                    .symbol_cache
                    .insert(word.clone(), fetched.clone());
                fetched
            };

            for sym in symbols {
                if is_credential_file(&sym.file_path) {
                    continue;
                }
                let key = format!("rank2:{}:{}", sym.file_path, sym.name);
                if seen_keys.insert(key.clone()) {
                    let formatted = format!(
                        "- [Direct Ref] Symbol: {} ({}) in {}",
                        sym.name, sym.kind, sym.file_path
                    );
                    let tokens = estimate_tokens(&formatted);
                    candidate_items.push(PriorityItem {
                        rank: 2,
                        key,
                        formatted,
                        tokens,
                        is_memory: false,
                    });
                }
            }
        }

        // Rank 3: Callers & Callees of active/referenced symbols
        for word in &query_words {
            telemetry.codegraph_queries_count += 1;
            if let Ok(report) = graph.get_blast_radius(word, 1) {
                if is_credential_file(&report.file_path) {
                    continue;
                }
                let key = format!("rank3:{}:{}", report.file_path, report.symbol);
                if seen_keys.insert(key.clone()) {
                    let formatted = format!(
                        "- [Caller/Callee] Symbol: {} ({}) in {}\n  References: {}",
                        report.symbol,
                        report.kind,
                        report.file_path,
                        report.references.len()
                    );
                    let tokens = estimate_tokens(&formatted);
                    candidate_items.push(PriorityItem {
                        rank: 3,
                        key,
                        formatted,
                        tokens,
                        is_memory: false,
                    });
                }
            }
        }

        // Rank 4: Direct imports & dependencies (symbols in visited files)
        for visited_file in visited_files {
            if is_credential_file(visited_file) {
                continue;
            }
            let clean_path = visited_file.replace('\\', "/");
            telemetry.codegraph_queries_count += 1;

            let file_symbols =
                if let Some(cached) = self.query_cache.file_symbol_cache.get(&clean_path) {
                    telemetry.cache_hits += 1;
                    cached.clone()
                } else {
                    telemetry.cache_misses += 1;
                    let fetched = graph.get_symbols_by_file(&clean_path).unwrap_or_default();
                    self.query_cache
                        .file_symbol_cache
                        .insert(clean_path.clone(), fetched.clone());
                    fetched
                };

            for sym in file_symbols {
                let key = format!("rank4:{}:{}", sym.file_path, sym.name);
                if seen_keys.insert(key.clone()) {
                    let formatted = format!(
                        "- [Dependency] Symbol: {} ({}) in {}",
                        sym.name, sym.kind, sym.file_path
                    );
                    let tokens = estimate_tokens(&formatted);
                    candidate_items.push(PriorityItem {
                        rank: 4,
                        key,
                        formatted,
                        tokens,
                        is_memory: false,
                    });
                }
            }
        }

        // Rank 5: Broader blast-radius candidate symbols
        for mod_file in modified_files {
            if is_credential_file(mod_file) {
                continue;
            }
            if let Some(stem) = std::path::Path::new(mod_file)
                .file_stem()
                .and_then(|s| s.to_str())
            {
                telemetry.codegraph_queries_count += 1;
                if let Ok(report) = graph.get_blast_radius(stem, 2) {
                    let key = format!("rank5:{}:{}", report.file_path, report.symbol);
                    if seen_keys.insert(key.clone()) {
                        let formatted = format!(
                            "- [Blast Radius] Candidate: {} ({}) in {}",
                            report.symbol, report.kind, report.file_path
                        );
                        let tokens = estimate_tokens(&formatted);
                        candidate_items.push(PriorityItem {
                            rank: 5,
                            key,
                            formatted,
                            tokens,
                            is_memory: false,
                        });
                    }
                }
            }
        }

        let symbol_candidate_count = candidate_items.len();
        telemetry.candidate_symbols_count = symbol_candidate_count;

        // Rank 6: Historical project/user memory entries (relevance-ranked)
        if let Some(mem_engine) = memory_engine {
            let active_files: Vec<String> = visited_files
                .iter()
                .chain(modified_files.iter())
                .cloned()
                .collect();

            if let Ok(memories) = mem_engine.recall_ranked(task, &active_files, &query_words, 10) {
                telemetry.memory_candidates_count = memories.len();
                for m in memories {
                    if is_credential_file(&m.key) || is_credential_file(&m.content) {
                        continue;
                    }
                    let scope = if m.is_global { "global" } else { "project" };
                    let formatted = format!(
                        "- [{}:{}] {}: {}",
                        scope,
                        m.category.as_str(),
                        m.key,
                        m.content
                    );
                    let key = format!("rank6:memory:{}", m.id);
                    let tokens = estimate_tokens(&formatted);
                    candidate_items.push(PriorityItem {
                        rank: 6,
                        key,
                        formatted,
                        tokens,
                        is_memory: true,
                    });
                }
            }
        }

        // Enforce 1,500 Token Hard Cap across Tier-3 context (with 500 token limit on personalization memory)
        const MAX_TIER3_TOKENS: usize = 1500;
        const MAX_PERSONALIZATION_MEMORY_TOKENS: usize = 500;
        let mut accumulated_tokens = 0;
        let mut accumulated_memory_tokens = 0;
        let mut retained_symbols = Vec::new();
        let mut retained_memories = Vec::new();
        let mut retained_symbol_count = 0;

        for item in candidate_items {
            if accumulated_tokens + item.tokens > MAX_TIER3_TOKENS {
                break;
            }
            if item.is_memory {
                if accumulated_memory_tokens + item.tokens > MAX_PERSONALIZATION_MEMORY_TOKENS {
                    continue;
                }
                accumulated_memory_tokens += item.tokens;
                accumulated_tokens += item.tokens;
                retained_memories.push(item.formatted);
            } else {
                accumulated_tokens += item.tokens;
                retained_symbols.push(item.formatted);
                retained_symbol_count += 1;
            }
        }

        telemetry.retained_symbols_count = retained_symbol_count;
        telemetry.tier3_tokens = accumulated_tokens;

        if !retained_symbols.is_empty() {
            self.ast_symbol_prompt = format!("## Symbol Context\n{}", retained_symbols.join("\n"));
        } else {
            self.ast_symbol_prompt.clear();
        }

        if !retained_memories.is_empty() {
            self.memory_prompt = format!(
                "## Persistent Memory Context\n{}",
                retained_memories.join("\n")
            );
        } else {
            self.memory_prompt.clear();
        }

        telemetry.context_build_latency_ms = start_time.elapsed().as_millis();
        telemetry
    }

    pub fn build_cache_aligned(&self) -> Result<CacheAlignedPrompt, String> {
        let compacted_messages = Compactor::compact_messages_with_checkpoint(
            &self.messages,
            self.max_context_tokens,
            self.checkpoint_summary.as_deref(),
        );

        let static_sys = if !self.static_system_prompt.is_empty() {
            &self.static_system_prompt
        } else {
            &self.system_prompt
        };

        let t1_tokens = estimate_tokens(static_sys);
        let t2_tokens = estimate_tokens(&self.tools_json.to_string());
        let t3_ast_tokens = estimate_tokens(&self.ast_symbol_prompt);
        let t3_mem_tokens = estimate_tokens(&self.memory_prompt);
        let t3_dyn_sys_tokens =
            if !self.static_system_prompt.is_empty() && !self.dynamic_system_prompt.is_empty() {
                estimate_tokens(&self.dynamic_system_prompt)
            } else {
                0
            };

        let static_prefix_tokens = t1_tokens + t2_tokens;

        let t4_tokens: usize = compacted_messages
            .iter()
            .map(|m| estimate_tokens(&m.content))
            .sum();

        let total_tokens =
            static_prefix_tokens + t3_ast_tokens + t3_mem_tokens + t3_dyn_sys_tokens + t4_tokens;

        let combined_tier3 = {
            let mut parts = Vec::new();
            if !self.static_system_prompt.is_empty() && !self.dynamic_system_prompt.is_empty() {
                parts.push(self.dynamic_system_prompt.clone());
            }
            if !self.ast_symbol_prompt.is_empty() {
                parts.push(self.ast_symbol_prompt.clone());
            }
            if !self.memory_prompt.is_empty() {
                parts.push(self.memory_prompt.clone());
            }
            parts.join("\n\n")
        };

        Ok(CacheAlignedPrompt {
            tier1_system: static_sys.clone(),
            tier2_tools: self.tools_json.clone(),
            tier3_ast_graph: combined_tier3,
            tier4_messages: compacted_messages,
            static_prefix_tokens,
            total_tokens,
        })
    }

    pub fn build(&self) -> Result<FormattedPrompt, String> {
        let cache_aligned = self.build_cache_aligned()?;

        let full_system_prompt = if cache_aligned.tier3_ast_graph.is_empty() {
            cache_aligned.tier1_system.clone()
        } else if cache_aligned.tier1_system.is_empty() {
            cache_aligned.tier3_ast_graph.clone()
        } else {
            format!(
                "{}\n\n{}",
                cache_aligned.tier1_system, cache_aligned.tier3_ast_graph
            )
        };

        Ok(FormattedPrompt {
            system_prompt: full_system_prompt,
            tools_json: cache_aligned.tier2_tools,
            messages: cache_aligned.tier4_messages,
            estimated_tokens: cache_aligned.total_tokens,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_credential_file() {
        assert!(is_credential_file(".env"));
        assert!(is_credential_file(".env.local"));
        assert!(is_credential_file(".env_test"));
        assert!(is_credential_file("config/.env"));
        assert!(!is_credential_file("src/main.rs"));
        assert!(!is_credential_file("environment.rs"));
    }

    #[test]
    fn test_cache_aligned_prefix_stability() {
        let mut builder = ContextBuilder::new(4000);
        builder.set_system_prompt("System policy v1");
        builder.set_tools_declaration(serde_json::json!({"tools": []}));
        builder.add_user_task("Task 1");

        let prompt = builder.build_cache_aligned().unwrap();
        assert!(prompt.static_prefix_tokens > 0);
        assert_eq!(prompt.tier1_system, "System policy v1");
        assert_eq!(prompt.tier4_messages.len(), 1);
        assert_eq!(prompt.tier4_messages[0].content, "Task 1");
    }

    #[test]
    fn test_reconstruct_from_checkpoint_preserves_state_and_sanitizes() {
        let mut builder = ContextBuilder::new(4000);

        // Populate pre-existing raw conversation history that MUST be cleared
        builder.add_user_task("Old task message 1");
        builder.add_assistant_message("Old raw assistant reasoning", None);
        builder.add_observation("read_file", "Old raw tool output");
        assert_eq!(builder.messages.len(), 3);

        let cp = TaskCheckpoint {
            task_id: "task_rec_123".to_string(),
            goal: "Refactor authentication module".to_string(),
            phase: "inspect".to_string(),
            completed_subtasks: vec!["subtask_completed_1".to_string()],
            pending_subtasks: vec!["subtask_pending_2".to_string()],
            modified_files: vec![
                "src/auth.rs".to_string(),
                ".env".to_string(),
                "id_rsa".to_string(),
            ],
            visited_files: vec!["src/lib.rs".to_string(), ".env.local".to_string()],
            executed_tools_summary: vec!["read_file (ok)".to_string()],
            verification_failures: vec!["auth_test failed line 42".to_string()],
            execution_strategy: Some("checkpointed:10:8".to_string()),
            ..TaskCheckpoint::default()
        };

        let notices = vec!["External file modification detected in src/lib.rs".to_string()];

        let _telemetry = builder.reconstruct_from_checkpoint(&cp, &notices, None, None);

        // 1. Raw conversation history cleared (no raw user/assistant/tool messages)
        let prompt = builder.build().unwrap();
        assert_eq!(
            prompt.messages.len(),
            1,
            "Should contain exactly 1 reconstructed prompt message"
        );
        let content = &prompt.messages[0].content;

        // 2. Goal, Phase, Subtasks, Failures preserved
        assert!(
            content.contains("Refactor authentication module"),
            "Goal missing"
        );
        assert!(content.contains("**Phase:** inspect"), "Phase missing");
        assert!(
            content.contains("- [x] subtask_completed_1"),
            "Completed subtask missing"
        );
        assert!(
            content.contains("- [ ] subtask_pending_2"),
            "Pending subtask missing"
        );
        assert!(
            content.contains("auth_test failed line 42"),
            "Verification failure missing"
        );
        assert!(
            content.contains("read_file (ok)"),
            "Executed tools summary missing"
        );
        assert!(
            content.contains("External file modification detected"),
            "Notice missing"
        );

        // 3. Files represented
        assert!(content.contains("src/auth.rs"), "Modified file missing");
        assert!(content.contains("src/lib.rs"), "Visited file missing");

        // 4. Credentials/sensitive files sanitized out
        assert!(!content.contains(".env"), "Credential file .env exposed!");
        assert!(
            !content.contains("id_rsa"),
            "Sensitive file id_rsa exposed!"
        );
        assert!(
            !content.contains(".env.local"),
            "Credential file .env.local exposed!"
        );

        // 5. Raw history not present
        assert!(!content.contains("Old raw assistant reasoning"));
        assert!(!content.contains("Old raw tool output"));
    }

    #[test]
    fn test_reconstruct_from_checkpoint_tier3_and_codegraph() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("graph.db");
        let mut graph = CodeGraph::open(&db_path).unwrap();

        let file_path = temp_dir.path().join("auth.rs");
        std::fs::write(
            &file_path,
            "pub fn login_user() -> bool { true }\npub fn logout_user() {}",
        )
        .unwrap();
        graph.index_workspace(temp_dir.path()).unwrap();

        let mut builder = ContextBuilder::new(4000);
        let cp = TaskCheckpoint {
            goal: "Refactor login_user function".to_string(),
            phase: "act".to_string(),
            visited_files: vec!["auth.rs".to_string()],
            ..TaskCheckpoint::default()
        };

        let _telemetry = builder.reconstruct_from_checkpoint(&cp, &[], Some(&graph), None);

        let prompt = builder.build_cache_aligned().unwrap();
        assert!(
            prompt.static_prefix_tokens <= 1500,
            "Tier-3 token hard cap exceeded!"
        );
    }

    #[test]
    fn test_dynamic_tier3_context_and_hard_cap() {
        let graph = CodeGraph::open_in_memory().unwrap();
        let mut builder = ContextBuilder::new(4000);

        let telemetry = builder.build_dynamic_tier3_context(
            &graph,
            "refactor auth module",
            &["src/auth.rs".to_string()],
            &["src/user.rs".to_string()],
            None,
        );

        assert!(telemetry.tier3_tokens <= 1500);
    }

    #[test]
    fn test_checkpoint_resume_context_reconstruction() {
        use chilli_memory::{CompactTaskTelemetry, TaskStatus, CURRENT_SCHEMA_VERSION};

        let cp = TaskCheckpoint {
            task_id: "task-res-1".to_string(),
            workspace_root: "/workspace".to_string(),
            workspace_hash: "hash123".to_string(),
            git_commit: Some("commit123".to_string()),
            goal: "Refactor auth persistence".to_string(),
            phase: "execute".to_string(),
            status: TaskStatus::Active,
            execution_strategy: Some("checkpointed:5:10".to_string()),
            completed_subtasks: vec!["Setup storage schema".to_string()],
            pending_subtasks: vec![
                "Implement atomic write".to_string(),
                "Run integration tests".to_string(),
            ],
            modified_files: vec!["src/storage.rs".to_string()],
            visited_files: vec!["src/lib.rs".to_string(), "src/storage.rs".to_string()],
            verification_attempts: 1,
            verification_failures: vec!["compilation error line 42".to_string()],
            executed_tools_summary: vec![
                "read: src/storage.rs".to_string(),
                "edit: src/storage.rs".to_string(),
            ],
            telemetry: CompactTaskTelemetry {
                input_tokens: 1000,
                output_tokens: 200,
                cached_tokens: 100,
                total_tokens: 1300,
                total_turns: 3,
                tool_calls_count: 2,
            },
            schema_version: CURRENT_SCHEMA_VERSION,
            created_at: 1000,
            updated_at: 1005,
        };

        let notices = vec!["External file modification detected in docs/readme.md".to_string()];
        let mut builder = ContextBuilder::new(4000);
        builder.set_system_prompt("System policy v1");
        builder.add_resumed_task_context(&cp, &notices);

        let prompt = builder.build_cache_aligned().unwrap();
        assert_eq!(prompt.tier4_messages.len(), 1);

        let content = &prompt.tier4_messages[0].content;
        assert!(content.contains("Refactor auth persistence"));
        assert!(content.contains("Setup storage schema"));
        assert!(content.contains("Implement atomic write"));
        assert!(content.contains("src/storage.rs"));
        assert!(content.contains("compilation error line 42"));
        assert!(content.contains("External file modification detected in docs/readme.md"));
        assert!(content.contains("Do NOT re-execute already completed tools"));
    }
}
