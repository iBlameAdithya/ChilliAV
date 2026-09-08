use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageItem {
    pub role: Role,
    pub content: String,
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tool_calls: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContextItem {
    System(String),
    ToolsDeclaration(serde_json::Value),
    UserTask(String),
    Message(MessageItem),
    Observation { name: String, output: String },
    RepoSymbols(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedPrompt {
    pub system_prompt: String,
    pub tools_json: serde_json::Value,
    pub messages: Vec<MessageItem>,
    pub estimated_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheAlignedPrompt {
    pub tier1_system: String,
    pub tier2_tools: serde_json::Value,
    pub tier3_ast_graph: String,
    pub tier4_messages: Vec<MessageItem>,
    pub static_prefix_tokens: usize,
    pub total_tokens: usize,
}

/// Simple heuristic token estimation: ~4 chars per token for English text & code
pub fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(4)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextEngineTelemetry {
    pub codegraph_queries_count: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub candidate_symbols_count: usize,
    pub retained_symbols_count: usize,
    pub tier3_tokens: usize,
    pub memory_candidates_count: usize,
    pub context_build_latency_ms: u128,
}

impl ContextEngineTelemetry {
    /// Calculate CodeGraph query cache hit rate ratio (0.0 to 1.0)
    pub fn cache_hit_rate(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total == 0 {
            0.0
        } else {
            self.cache_hits as f64 / total as f64
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenTelemetry {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cached_tokens: usize,
    pub tool_output_tokens: usize,
    pub compaction_saved_tokens: usize,
    pub total_task_tokens: usize,
    pub context_engine: ContextEngineTelemetry,
}

impl TokenTelemetry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update_total(&mut self) {
        self.total_task_tokens = self.input_tokens + self.output_tokens;
    }

    /// Calculate prompt cache hit rate ratio (0.0 to 1.0)
    pub fn prompt_cache_hit_rate(&self) -> f64 {
        let total_input = self.input_tokens + self.cached_tokens;
        if total_input == 0 {
            0.0
        } else {
            self.cached_tokens as f64 / total_input as f64
        }
    }
}
