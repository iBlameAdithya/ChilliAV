use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolCallChunk {
    pub id: String,
    pub name: String,
    pub arguments_json: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
}

impl ToolCallChunk {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments_json: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            arguments_json: arguments_json.into(),
            index: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TokenUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reasoning_tokens: Option<usize>,
    pub cached_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StreamEvent {
    TextChunk(String),
    ToolCallChunk(ToolCallChunk),
    Usage(TokenUsage),
    Finished(StopReason),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompletionRequest {
    pub prompt: String,
    pub system_prompt: Option<String>,
    pub tools_json: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub messages: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ModelResponse {
    pub text_content: String,
    pub tool_calls: Vec<ToolCallChunk>,
    pub token_usage: TokenUsage,
    pub stop_reason: Option<StopReason>,
}

#[derive(Debug, Clone, Default)]
pub struct StreamAccumulator {
    pub text_content: String,
    pub tool_calls: Vec<ToolCallChunk>,
    pub token_usage: TokenUsage,
    pub stop_reason: Option<StopReason>,
}

impl StreamAccumulator {
    pub fn process(&mut self, event: StreamEvent) {
        match event {
            StreamEvent::TextChunk(text) => self.text_content.push_str(&text),
            StreamEvent::ToolCallChunk(call) => self.tool_calls.push(call),
            StreamEvent::Usage(usage) => self.token_usage = usage,
            StreamEvent::Finished(reason) => {
                if self.stop_reason.is_none() {
                    self.stop_reason = Some(reason);
                }
            }
        }
    }

    pub fn into_response(self) -> ModelResponse {
        ModelResponse {
            text_content: self.text_content,
            tool_calls: self.tool_calls,
            token_usage: self.token_usage,
            stop_reason: self.stop_reason,
        }
    }
}

#[async_trait]
pub trait ModelAdapter: Send + Sync {
    fn provider_name(&self) -> &'static str;
    fn model_name(&self) -> &str {
        "unknown"
    }
    fn parse_sse_data(&self, json_data: &str) -> Result<Vec<StreamEvent>, String>;
    fn parse_sse_event(
        &self,
        event: &crate::transport::SseEvent,
    ) -> Result<Vec<StreamEvent>, String> {
        self.parse_sse_data(&event.data)
    }
    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> Result<tokio_stream::wrappers::ReceiverStream<StreamEvent>, String> {
        Err("Complete not implemented for adapter".to_string())
    }
}
