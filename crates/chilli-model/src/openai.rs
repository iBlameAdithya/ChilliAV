use crate::adapter::{
    CompletionRequest, ModelAdapter, StopReason, StreamEvent, TokenUsage, ToolCallChunk,
};
use async_trait::async_trait;
use serde_json::Value;
use std::env;

#[derive(Debug, Clone)]
pub struct OpenAIAdapter {
    pub api_key: String,
    pub model_name: String,
    pub base_url: String,
}

pub use OpenAIAdapter as OpenAiAdapter;

impl Default for OpenAIAdapter {
    fn default() -> Self {
        Self::from_env()
    }
}

impl OpenAIAdapter {
    pub fn new(api_key: String, model_name: String, base_url: String) -> Self {
        Self {
            api_key,
            model_name,
            base_url,
        }
    }

    pub fn from_env() -> Self {
        Self::from_env_with_model(None)
    }

    pub fn from_env_with_model(model_override: Option<&str>) -> Self {
        let override_str = model_override.unwrap_or("").trim();
        let lower = override_str.to_lowercase();
        let target_key = crate::find_target_api_key();

        let is_valid_override =
            !override_str.is_empty() && override_str != "groq" && override_str != "openai";

        // 1. Explicit Local / Ollama / Local LLM override or env base URL
        if is_valid_override
            && (lower == "local"
                || lower == "ollama"
                || lower == "ollama-local"
                || lower == "lmstudio"
                || lower == "vllm"
                || lower.starts_with("qwen")
                || lower.starts_with("ollama/")
                || lower.contains(":1.")
                || lower.contains(":7b")
                || lower.contains(":8b"))
        {
            let url = env::var("OPENAI_BASE_URL")
                .or_else(|_| env::var("OLLAMA_BASE_URL"))
                .or_else(|_| env::var("OLLAMA_HOST"))
                .unwrap_or_else(|_| "http://localhost:11434/v1".to_string());
            let api_key = env::var("OPENAI_API_KEY").unwrap_or_else(|_| "ollama-local".to_string());
            let model = if lower == "local" || lower == "ollama" || lower == "ollama-local" {
                env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3".to_string())
            } else {
                override_str.to_string()
            };
            return Self::new(api_key, model, url);
        }

        if let Ok(url) = env::var("OPENAI_BASE_URL")
            .or_else(|_| env::var("OLLAMA_BASE_URL"))
            .or_else(|_| env::var("LOCAL_LLM_URL"))
            .or_else(|_| env::var("CHILLI_EVAL_BASE_URL"))
        {
            if !url.trim().is_empty() {
                let model = if is_valid_override {
                    override_str.to_string()
                } else {
                    env::var("OLLAMA_MODEL")
                        .or_else(|_| env::var("CHILLI_EVAL_MODEL"))
                        .unwrap_or_else(|_| "llama3".to_string())
                };
                let api_key =
                    env::var("OPENAI_API_KEY").unwrap_or_else(|_| "ollama-local".to_string());
                return Self::new(api_key, model, url);
            }
        }

        if let Ok(azure_key) = env::var("AZURE_OPENAI_API_KEY")
            .or_else(|_| env::var("AZURE_API_KEY"))
        {
            let endpoint = env::var("AZURE_OPENAI_ENDPOINT")
                .or_else(|_| env::var("AZURE_ENDPOINT"))
                .unwrap_or_else(|_| "https://nakuladithya77-1660-resource.services.ai.azure.com".to_string());
            let base_url = if endpoint.contains("/openai/v1") {
                endpoint
            } else {
                format!("{}/openai/v1", endpoint.trim_end_matches('/'))
            };
            let model = if is_valid_override {
                override_str.to_string()
            } else {
                env::var("AZURE_OPENAI_MODEL")
                    .or_else(|_| env::var("AZURE_MODEL"))
                    .unwrap_or_else(|_| "gpt-5.6-sol-1".to_string())
            };
            return Self::new(azure_key, model, base_url);
        }

        if is_valid_override
            && !lower.starts_with("openai/")
            && (lower.starts_with("gpt") || lower.starts_with("o1") || lower.starts_with("o3"))
        {
            let api_key = env::var("OPENAI_API_KEY")
                .ok()
                .or_else(|| target_key.clone())
                .unwrap_or_default();
            let base_url = env::var("OPENAI_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
            return Self::new(api_key, override_str.to_string(), base_url);
        }

        if is_valid_override
            && !lower.contains("fireworks")
            && !lower.starts_with("accounts/fireworks/")
            && !lower.contains("cerebras")
            && !lower.starts_with("openai/")
            && (lower.starts_with("llama")
                || lower.starts_with("mixtral")
                || lower.starts_with("gemma")
                || lower.starts_with("deepseek")
                || lower.starts_with("qwen"))
            && (env::var("GROQ_API_KEY").is_ok()
                || (env::var("OPENROUTER_API_KEY").is_err()
                    && env::var("FIREWORKS_API_KEY").is_err()
                    && env::var("CEREBRAS_API_KEY").is_err()
                    && target_key.as_ref().is_none_or(|k| {
                        !k.starts_with("sk-or-") && !k.starts_with("fw_") && !k.starts_with("csk-")
                    })))
        {
            let groq_key = env::var("GROQ_API_KEY").unwrap_or_default();
            let base_url = env::var("GROQ_BASE_URL")
                .unwrap_or_else(|_| "https://api.groq.com/openai/v1".to_string());
            return Self::new(groq_key, override_str.to_string(), base_url);
        }

        if is_valid_override && lower.starts_with("gemini") && !lower.starts_with("google/") {
            let key = env::var("GEMINI_API_KEY")
                .or_else(|_| env::var("GOOGLE_API_KEY"))
                .ok()
                .or_else(|| target_key.clone())
                .unwrap_or_default();
            let base_url = env::var("GEMINI_BASE_URL")
                .or_else(|_| env::var("GOOGLE_BASE_URL"))
                .unwrap_or_else(|_| {
                    "https://generativelanguage.googleapis.com/v1beta/openai".to_string()
                });
            return Self::new(key, override_str.to_string(), base_url);
        }

        if std::env::var("OPENROUTER_API_KEY").is_ok()
            || lower.contains("openrouter")
            || lower.starts_with("openai/")
            || lower.starts_with("google/")
            || target_key.as_ref().is_some_and(|k| k.starts_with("sk-or-"))
        {
            let key = std::env::var("OPENROUTER_API_KEY")
                .ok()
                .or_else(|| {
                    target_key
                        .as_ref()
                        .filter(|k| k.starts_with("sk-or-"))
                        .cloned()
                })
                .or_else(|| target_key.clone())
                .unwrap_or_default();
            let base_url = env::var("OPENROUTER_BASE_URL")
                .unwrap_or_else(|_| "https://openrouter.ai/api/v1".to_string());
            let model = if is_valid_override {
                override_str.to_string()
            } else {
                env::var("CHILLI_EVAL_MODEL").unwrap_or_else(|_| "openai/gpt-4o-mini".to_string())
            };
            return Self::new(key, model, base_url);
        }

        if std::env::var("CEREBRAS_API_KEY").is_ok()
            || lower.contains("cerebras")
            || target_key.as_ref().is_some_and(|k| k.starts_with("csk-"))
        {
            let key = std::env::var("CEREBRAS_API_KEY")
                .ok()
                .or_else(|| {
                    target_key
                        .as_ref()
                        .filter(|k| k.starts_with("csk-"))
                        .cloned()
                })
                .or_else(|| target_key.clone())
                .unwrap_or_default();
            let base_url = env::var("CEREBRAS_BASE_URL")
                .unwrap_or_else(|_| "https://api.cerebras.ai/v1".to_string());
            let model = if is_valid_override {
                override_str.to_string()
            } else {
                "llama-3.3-70b".to_string()
            };
            return Self::new(key, model, base_url);
        }

        if std::env::var("FIREWORKS_API_KEY").is_ok()
            || lower.contains("fireworks")
            || lower.starts_with("accounts/fireworks/")
            || target_key.as_ref().is_some_and(|k| k.starts_with("fw_"))
        {
            let key = std::env::var("FIREWORKS_API_KEY")
                .ok()
                .or_else(|| {
                    target_key
                        .as_ref()
                        .filter(|k| k.starts_with("fw_"))
                        .cloned()
                })
                .or_else(|| target_key.clone())
                .unwrap_or_default();
            let base_url = env::var("FIREWORKS_BASE_URL")
                .unwrap_or_else(|_| "https://api.fireworks.ai/inference/v1".to_string());
            let model = if is_valid_override {
                override_str.to_string()
            } else {
                "accounts/fireworks/models/deepseek-v4-pro".to_string()
            };
            return Self::new(key, model, base_url);
        }

        if is_valid_override
            && (lower.starts_with("gpt") || lower.starts_with("o1") || lower.starts_with("o3"))
        {
            let api_key = env::var("OPENAI_API_KEY")
                .ok()
                .or_else(|| target_key.clone())
                .unwrap_or_default();
            let base_url = env::var("OPENAI_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
            return Self::new(api_key, override_str.to_string(), base_url);
        }

        if is_valid_override
            && (lower.starts_with("llama")
                || lower.starts_with("mixtral")
                || lower.starts_with("gemma")
                || lower.starts_with("deepseek")
                || lower.starts_with("qwen"))
        {
            let groq_key = env::var("GROQ_API_KEY").unwrap_or_default();
            let base_url = env::var("GROQ_BASE_URL")
                .unwrap_or_else(|_| "https://api.groq.com/openai/v1".to_string());
            return Self::new(groq_key, override_str.to_string(), base_url);
        }

        if is_valid_override && lower.starts_with("gemini") {
            let key = env::var("GEMINI_API_KEY")
                .or_else(|_| env::var("GOOGLE_API_KEY"))
                .ok()
                .or_else(|| target_key.clone())
                .unwrap_or_default();
            let base_url = env::var("GEMINI_BASE_URL")
                .or_else(|_| env::var("GOOGLE_BASE_URL"))
                .unwrap_or_else(|_| {
                    "https://generativelanguage.googleapis.com/v1beta/openai".to_string()
                });
            return Self::new(key, override_str.to_string(), base_url);
        }

        if (env::var("GEMINI_API_KEY").is_ok() || env::var("GOOGLE_API_KEY").is_ok())
            && env::var("GROQ_API_KEY").is_err()
            && env::var("OPENAI_API_KEY").is_err()
            && env::var("OPENROUTER_API_KEY").is_err()
        {
            let key = env::var("GEMINI_API_KEY")
                .or_else(|_| env::var("GOOGLE_API_KEY"))
                .unwrap_or_default();
            let model = if is_valid_override {
                override_str.to_string()
            } else {
                env::var("GEMINI_MODEL")
                    .or_else(|_| env::var("GOOGLE_MODEL"))
                    .unwrap_or_else(|_| "gemini-3.6-flash".to_string())
            };
            let base_url = env::var("GEMINI_BASE_URL")
                .or_else(|_| env::var("GOOGLE_BASE_URL"))
                .unwrap_or_else(|_| {
                    "https://generativelanguage.googleapis.com/v1beta/openai".to_string()
                });
            return Self::new(key, model, base_url);
        }

        if let Ok(azure_key) = env::var("AZURE_OPENAI_API_KEY")
            .or_else(|_| env::var("AZURE_API_KEY"))
        {
            let base_url = env::var("AZURE_OPENAI_ENDPOINT")
                .or_else(|_| env::var("AZURE_ENDPOINT"))
                .unwrap_or_else(|_| "https://nakuladithya77-1660-resource.services.ai.azure.com/openai/v1".to_string());
            let model = if is_valid_override {
                override_str.to_string()
            } else {
                env::var("AZURE_OPENAI_MODEL")
                    .or_else(|_| env::var("AZURE_MODEL"))
                    .unwrap_or_else(|_| "gpt-4.1-mini".to_string())
            };
            return Self::new(azure_key, model, base_url);
        }

        if let Ok(groq_key) = env::var("GROQ_API_KEY") {
            let model = if is_valid_override {
                override_str.to_string()
            } else {
                env::var("GROQ_MODEL").unwrap_or_else(|_| "llama-3.3-70b-versatile".to_string())
            };
            let base_url = env::var("GROQ_BASE_URL")
                .unwrap_or_else(|_| "https://api.groq.com/openai/v1".to_string());
            Self::new(groq_key, model, base_url)
        } else {
            let api_key = env::var("OPENAI_API_KEY")
                .ok()
                .or(target_key)
                .unwrap_or_default();
            let model = if is_valid_override {
                override_str.to_string()
            } else {
                env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o".to_string())
            };
            let base_url = env::var("OPENAI_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
            Self::new(api_key, model, base_url)
        }
    }

    pub fn for_groq(api_key: String) -> Self {
        Self::new(
            api_key,
            "llama-3.3-70b-versatile".to_string(),
            "https://api.groq.com/openai/v1".to_string(),
        )
    }

    pub fn for_gemini(api_key: String) -> Self {
        Self::new(
            api_key,
            "gemini-3.6-flash".to_string(),
            "https://generativelanguage.googleapis.com/v1beta/openai".to_string(),
        )
    }

    pub fn for_gemini_with_model(api_key: String, model: String) -> Self {
        Self::new(
            api_key,
            model,
            "https://generativelanguage.googleapis.com/v1beta/openai".to_string(),
        )
    }

    pub fn for_azure(api_key: String, endpoint: String) -> Self {
        let base_url = if endpoint.contains("/openai/v1") {
            endpoint
        } else {
            format!("{}/openai/v1", endpoint.trim_end_matches('/'))
        };
        Self::new(api_key, "gpt-4.1-mini".to_string(), base_url)
    }

    pub fn for_azure_with_model(api_key: String, endpoint: String, model: String) -> Self {
        let base_url = if endpoint.contains("/openai/v1") {
            endpoint
        } else {
            format!("{}/openai/v1", endpoint.trim_end_matches('/'))
        };
        Self::new(api_key, model, base_url)
    }
}

#[async_trait]
impl ModelAdapter for OpenAIAdapter {
    fn provider_name(&self) -> &'static str {
        if self.base_url.contains("azure") || self.base_url.contains("services.ai.azure.com") {
            "azure"
        } else if self.base_url.contains("groq") {
            "groq"
        } else if self.base_url.contains("openrouter") {
            "openrouter"
        } else if self.base_url.contains("deepseek") {
            "deepseek"
        } else if self.base_url.contains("googleapis.com") || self.base_url.contains("gemini") {
            "gemini"
        } else {
            "openai"
        }
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    fn parse_sse_data(&self, json_data: &str) -> Result<Vec<StreamEvent>, String> {
        let v: Value = match serde_json::from_str(json_data) {
            Ok(val) => val,
            Err(_) => return Ok(Vec::new()),
        };

        let mut events = Vec::new();

        if let Some(choices) = v.get("choices").and_then(|c| c.as_array()) {
            for choice in choices {
                if let Some(delta) = choice.get("delta") {
                    let text_chunk = delta
                        .get("content")
                        .or_else(|| delta.get("reasoning"))
                        .or_else(|| delta.get("reasoning_content"))
                        .and_then(|s| s.as_str());

                    if let Some(content) = text_chunk {
                        if !content.is_empty() {
                            events.push(StreamEvent::TextChunk(content.to_string()));
                        }
                    }

                    if let Some(tool_calls) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                        for tc in tool_calls {
                            let index =
                                tc.get("index").and_then(|i| i.as_u64()).map(|i| i as usize);
                            let id = tc
                                .get("id")
                                .and_then(|s| s.as_str())
                                .unwrap_or("")
                                .to_string();
                            let name = tc
                                .get("function")
                                .and_then(|f| f.get("name"))
                                .and_then(|s| s.as_str())
                                .unwrap_or("")
                                .to_string();
                            let arguments = tc
                                .get("function")
                                .and_then(|f| f.get("arguments"))
                                .and_then(|s| s.as_str())
                                .unwrap_or("")
                                .to_string();

                            events.push(StreamEvent::ToolCallChunk(ToolCallChunk {
                                id,
                                name,
                                arguments_json: arguments,
                                index,
                            }));
                        }
                    }
                }

                if let Some(finish_reason) = choice.get("finish_reason").and_then(|s| s.as_str()) {
                    let reason = match finish_reason {
                        "stop" => StopReason::EndTurn,
                        "tool_calls" => StopReason::ToolUse,
                        "length" => StopReason::MaxTokens,
                        other => StopReason::Error(other.to_string()),
                    };
                    events.push(StreamEvent::Finished(reason));
                }
            }
        }

        if let Some(usage) = v.get("usage") {
            let prompt_tokens = usage
                .get("prompt_tokens")
                .and_then(|n| n.as_u64())
                .unwrap_or(0) as usize;
            let completion_tokens = usage
                .get("completion_tokens")
                .and_then(|n| n.as_u64())
                .unwrap_or(0) as usize;
            let reasoning_tokens = usage
                .get("completion_tokens_details")
                .and_then(|d| d.get("reasoning_tokens"))
                .and_then(|n| n.as_u64())
                .map(|n| n as usize);
            let cached_tokens = usage
                .get("prompt_tokens_details")
                .and_then(|d| d.get("cached_tokens"))
                .and_then(|n| n.as_u64())
                .unwrap_or(0) as usize;

            events.push(StreamEvent::Usage(TokenUsage {
                prompt_tokens,
                completion_tokens,
                reasoning_tokens,
                cached_tokens,
            }));
        }

        Ok(events)
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<tokio_stream::wrappers::ReceiverStream<StreamEvent>, String> {
        let client = reqwest::Client::new();

        let messages = if let Some(msgs) = &request.messages {
            msgs.clone()
        } else {
            let mut msgs = Vec::new();
            if let Some(sys) = &request.system_prompt {
                if !sys.is_empty() {
                    msgs.push(serde_json::json!({
                        "role": "system",
                        "content": sys
                    }));
                }
            }
            msgs.push(serde_json::json!({
                "role": "user",
                "content": request.prompt
            }));
            msgs
        };

        let max_toks = request.max_tokens.unwrap_or(8192);

        let lower_model = self.model_name.to_lowercase();
        let is_next_gen_or_reasoning = lower_model.starts_with("gpt-5")
            || lower_model.starts_with("o1")
            || lower_model.starts_with("o3")
            || lower_model.starts_with("o4");

        let mut body = if is_next_gen_or_reasoning {
            serde_json::json!({
                "model": self.model_name,
                "messages": messages,
                "max_completion_tokens": max_toks,
                "stream": true,
            })
        } else {
            serde_json::json!({
                "model": self.model_name,
                "messages": messages,
                "max_tokens": max_toks,
                "stream": true,
            })
        };

        if !self.base_url.contains("nvidia") {
            body["stream_options"] = serde_json::json!({
                "include_usage": true
            });
        }

        if let Some(tools) = &request.tools_json {
            if let Some(arr) = tools.as_array() {
                if !arr.is_empty() {
                    let formatted_tools: Vec<serde_json::Value> = arr
                        .iter()
                        .map(|t| {
                            let name = t.get("name").and_then(|n| n.as_str()).unwrap_or("");
                            let desc = t.get("description").and_then(|d| d.as_str()).unwrap_or("");
                            let params = if let Some(p) = t.get("parameters") {
                                p.clone()
                            } else {
                                match name {
                                    "read_file" => serde_json::json!({
                                        "type": "object",
                                        "properties": {
                                            "path": { "type": "string", "description": "Relative file path" },
                                            "offset": { "type": "integer", "description": "Optional starting line offset (1-based)" },
                                            "limit": { "type": "integer", "description": "Optional line count limit" }
                                        },
                                        "required": ["path"]
                                    }),
                                    "write_file" => serde_json::json!({
                                        "type": "object",
                                        "properties": {
                                            "path": { "type": "string", "description": "Relative file path" },
                                            "content": { "type": "string", "description": "File content to write" }
                                        },
                                        "required": ["path", "content"]
                                    }),
                                    "edit_file" => serde_json::json!({
                                        "type": "object",
                                        "properties": {
                                            "path": { "type": "string", "description": "Relative file path" },
                                            "old_string": { "type": "string", "description": "Exact text snippet to replace" },
                                            "new_string": { "type": "string", "description": "New replacement text snippet" }
                                        },
                                        "required": ["path", "old_string", "new_string"]
                                    }),
                                    "grep" => serde_json::json!({
                                        "type": "object",
                                        "properties": {
                                            "pattern": { "type": "string", "description": "Search pattern" }
                                        },
                                        "required": ["pattern"]
                                    }),
                                    "exec" => serde_json::json!({
                                        "type": "object",
                                        "properties": {
                                            "command": { "type": "string", "description": "Shell command executable or pipeline" },
                                            "args": {
                                                "type": "array",
                                                "items": { "type": "string" },
                                                "description": "Optional command arguments"
                                            },
                                            "cwd": { "type": "string", "description": "Optional relative working directory" },
                                            "timeout_ms": { "type": "integer", "description": "Optional timeout in milliseconds" }
                                        },
                                        "required": ["command"]
                                    }),
                                    "move_file" => serde_json::json!({
                                        "type": "object",
                                        "properties": {
                                            "from": { "type": "string", "description": "Source path relative to workspace root" },
                                            "to": { "type": "string", "description": "Destination path relative to workspace root" }
                                        },
                                        "required": ["from", "to"]
                                    }),
                                    "copy_file" => serde_json::json!({
                                        "type": "object",
                                        "properties": {
                                            "from": { "type": "string", "description": "Source path relative to workspace root" },
                                            "to": { "type": "string", "description": "Destination path relative to workspace root" }
                                        },
                                        "required": ["from", "to"]
                                    }),
                                    "create_directory" => serde_json::json!({
                                        "type": "object",
                                        "properties": {
                                            "path": { "type": "string", "description": "Relative directory path to create" }
                                        },
                                        "required": ["path"]
                                    }),
                                    "remove_file" => serde_json::json!({
                                        "type": "object",
                                        "properties": {
                                            "path": { "type": "string", "description": "Relative path of file or directory to remove" }
                                        },
                                        "required": ["path"]
                                    }),
                                    _ => serde_json::json!({
                                        "type": "object",
                                        "properties": {
                                            "path": { "type": "string", "description": "File path" },
                                            "content": { "type": "string", "description": "File content" },
                                            "pattern": { "type": "string", "description": "Search pattern" },
                                            "command": { "type": "string", "description": "Shell command" }
                                        }
                                    }),
                                }
                            };

                            serde_json::json!({
                                "type": "function",
                                "function": {
                                    "name": name,
                                    "description": desc,
                                    "parameters": params
                                }
                            })
                        })
                        .collect();
                    body["tools"] = serde_json::json!(formatted_tools);
                }
            }
        }

        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let is_azure = self.base_url.contains("azure") || self.base_url.contains("services.ai.azure.com");

        let mut retries = 0;
        let resp = loop {
            let mut req_builder = client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json");

            if is_azure {
                req_builder = req_builder.header("api-key", &self.api_key);
            }

            let resp = req_builder
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("HTTP request failed: {}", e))?;

            let status = resp.status();
            if (status == reqwest::StatusCode::TOO_MANY_REQUESTS
                || status == reqwest::StatusCode::GATEWAY_TIMEOUT
                || status == reqwest::StatusCode::BAD_GATEWAY
                || status == reqwest::StatusCode::SERVICE_UNAVAILABLE)
                && retries < 10
            {
                retries += 1;
                tokio::time::sleep(std::time::Duration::from_millis(3000 * retries as u64)).await;
                continue;
            }

            if !resp.status().is_success() {
                let status = resp.status();
                let err_text = resp.text().await.unwrap_or_default();
                let provider = self.provider_name();
                return Err(format!(
                    "API error {} from provider '{}' ({}) for model '{}': {}",
                    status, provider, self.base_url, self.model_name, err_text
                ));
            }
            break resp;
        };

        let (tx, rx) = tokio::sync::mpsc::channel(100);

        let adapter = self.clone();
        tokio::spawn(async move {
            use futures::StreamExt;
            let mut sse_stream = crate::transport::SseTransport::stream_bytes(resp);

            while let Some(item) = sse_stream.next().await {
                match item {
                    Ok(sse_event) => {
                        if sse_event.data == "[DONE]" {
                            break;
                        }
                        match adapter.parse_sse_event(&sse_event) {
                            Ok(events) => {
                                for ev in events {
                                    if tx.send(ev).await.is_err() {
                                        return;
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!(
                                    "[OPENAI ADAPTER SSE PARSE ERROR] data='{}', err='{}'",
                                    sse_event.data, e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(StreamEvent::Finished(StopReason::Error(e))).await;
                        break;
                    }
                }
            }
        });

        Ok(tokio_stream::wrappers::ReceiverStream::new(rx))
    }
}
