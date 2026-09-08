use async_trait::async_trait;
use serde_json::Value;
use std::env;

use crate::adapter::{
    CompletionRequest, ModelAdapter, StopReason, StreamEvent, TokenUsage, ToolCallChunk,
};

#[derive(Debug, Clone)]
pub struct AnthropicAdapter {
    pub api_key: String,
    pub model_name: String,
    pub base_url: String,
}

impl Default for AnthropicAdapter {
    fn default() -> Self {
        Self::from_env()
    }
}

impl AnthropicAdapter {
    pub fn new(api_key: String, model_name: String, base_url: String) -> Self {
        let trimmed_url = base_url.trim_end_matches('/');
        let normalized_base_url = if !trimmed_url.ends_with("/v1")
            && (trimmed_url.contains("8080")
                || trimmed_url.contains("127.0.0.1")
                || trimmed_url.starts_with("http"))
        {
            format!("{}/v1", trimmed_url)
        } else {
            trimmed_url.to_string()
        };
        Self {
            api_key,
            model_name,
            base_url: normalized_base_url,
        }
    }

    pub fn from_env() -> Self {
        Self::from_env_with_model(None)
    }

    pub fn from_env_with_model(model_override: Option<&str>) -> Self {
        let is_agcl_env = env::var("AGCL_BASE_URL").is_ok() || env::var("AGCL_MODEL").is_ok();
        let override_str = model_override.unwrap_or("").trim();
        let lower = override_str.to_lowercase();
        let is_agcl_override = lower == "agcl"
            || lower == "ag-cl"
            || lower.starts_with("agcl/")
            || lower.starts_with("ag-cl/");

        let model = if !override_str.is_empty() {
            if is_agcl_override {
                if lower == "agcl" || lower == "ag-cl" {
                    env::var("AGCL_MODEL").unwrap_or_else(|_| "gemini-3.6-flash-medium".to_string())
                } else if lower.starts_with("agcl/") {
                    override_str[5..].to_string()
                } else {
                    override_str[6..].to_string()
                }
            } else {
                override_str.to_string()
            }
        } else if let Ok(agcl_model) = env::var("AGCL_MODEL") {
            agcl_model
        } else {
            env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-3-5-sonnet-20241022".to_string())
        };

        let base_url = if is_agcl_override || is_agcl_env {
            env::var("AGCL_BASE_URL").unwrap_or_else(|_| "http://localhost:8080".to_string())
        } else {
            env::var("ANTHROPIC_BASE_URL")
                .unwrap_or_else(|_| "https://api.anthropic.com/v1".to_string())
        };

        let api_key = if is_agcl_override || is_agcl_env {
            env::var("AGCL_API_KEY")
                .or_else(|_| env::var("ANTHROPIC_API_KEY"))
                .unwrap_or_else(|_| "agcl-local-key".to_string())
        } else {
            env::var("ANTHROPIC_API_KEY").unwrap_or_default()
        };

        Self::new(api_key, model, base_url)
    }
}

#[async_trait]
impl ModelAdapter for AnthropicAdapter {
    fn provider_name(&self) -> &'static str {
        if self.base_url.contains("8080") || self.model_name.starts_with("gemini") {
            "agcl"
        } else {
            "anthropic"
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

        if let Some(event_type) = v.get("type").and_then(|t| t.as_str()) {
            let index = v.get("index").and_then(|i| i.as_u64()).map(|i| i as usize);
            match event_type {
                "content_block_start" => {
                    if let Some(block) = v.get("content_block") {
                        if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                            let id = block
                                .get("id")
                                .and_then(|s| s.as_str())
                                .unwrap_or("")
                                .to_string();
                            let name = block
                                .get("name")
                                .and_then(|s| s.as_str())
                                .unwrap_or("")
                                .to_string();
                            events.push(StreamEvent::ToolCallChunk(ToolCallChunk {
                                id,
                                name,
                                arguments_json: String::new(),
                                index,
                            }));
                        }
                    }
                }
                "content_block_delta" => {
                    if let Some(delta) = v.get("delta") {
                        if let Some(delta_type) = delta.get("type").and_then(|t| t.as_str()) {
                            match delta_type {
                                "text_delta" => {
                                    if let Some(text) = delta.get("text").and_then(|s| s.as_str()) {
                                        events.push(StreamEvent::TextChunk(text.to_string()));
                                    }
                                }
                                "input_json_delta" => {
                                    if let Some(partial_json) =
                                        delta.get("partial_json").and_then(|s| s.as_str())
                                    {
                                        events.push(StreamEvent::ToolCallChunk(ToolCallChunk {
                                            id: String::new(),
                                            name: String::new(),
                                            arguments_json: partial_json.to_string(),
                                            index,
                                        }));
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                "message_delta" => {
                    if let Some(delta) = v.get("delta") {
                        if let Some(stop_reason_str) =
                            delta.get("stop_reason").and_then(|s| s.as_str())
                        {
                            let reason = match stop_reason_str {
                                "end_turn" => StopReason::EndTurn,
                                "tool_use" => StopReason::ToolUse,
                                "max_tokens" => StopReason::MaxTokens,
                                other => StopReason::Error(other.to_string()),
                            };
                            events.push(StreamEvent::Finished(reason));
                        }
                    }
                    if let Some(usage) = v.get("usage") {
                        let completion_tokens = usage
                            .get("output_tokens")
                            .and_then(|n| n.as_u64())
                            .unwrap_or(0) as usize;
                        events.push(StreamEvent::Usage(TokenUsage {
                            prompt_tokens: 0,
                            completion_tokens,
                            reasoning_tokens: None,
                            cached_tokens: 0,
                        }));
                    }
                }
                "message_start" => {
                    if let Some(msg) = v.get("message") {
                        if let Some(usage) = msg.get("usage") {
                            let prompt_tokens = usage
                                .get("input_tokens")
                                .and_then(|n| n.as_u64())
                                .unwrap_or(0)
                                as usize;
                            let cached_tokens = usage
                                .get("cache_read_input_tokens")
                                .and_then(|n| n.as_u64())
                                .unwrap_or(0)
                                as usize;
                            events.push(StreamEvent::Usage(TokenUsage {
                                prompt_tokens,
                                completion_tokens: 0,
                                reasoning_tokens: None,
                                cached_tokens,
                            }));
                        }
                    }
                }
                "message_stop" => {
                    // Stop reason is already provided by message_delta; message_stop is the stream completion marker.
                }
                _ => {}
            }
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
            vec![serde_json::json!({
                "role": "user",
                "content": request.prompt
            })]
        };

        let max_toks = request.max_tokens.unwrap_or(8192);

        let mut body = serde_json::json!({
            "model": self.model_name,
            "max_tokens": max_toks,
            "messages": messages,
            "stream": true,
        });

        if let Some(sys) = &request.system_prompt {
            if !sys.is_empty() {
                body["system"] = serde_json::json!(sys);
            }
        }

        if let Some(tools) = &request.tools_json {
            if let Some(arr) = tools.as_array() {
                if !arr.is_empty() {
                    let formatted_tools: Vec<serde_json::Value> = arr
                        .iter()
                        .map(|t| {
                            let name = t.get("name").and_then(|n| n.as_str()).unwrap_or("");
                            let desc = t.get("description").and_then(|d| d.as_str()).unwrap_or("");
                            let input_schema = t.get("parameters").cloned().unwrap_or_else(|| {
                                serde_json::json!({
                                    "type": "object",
                                    "properties": {}
                                })
                            });
                            serde_json::json!({
                                "name": name,
                                "description": desc,
                                "input_schema": input_schema
                            })
                        })
                        .collect();
                    body["tools"] = serde_json::json!(formatted_tools);
                }
            }
        }

        let url = format!("{}/messages", self.base_url.trim_end_matches('/'));

        let mut retries = 0;
        let resp = loop {
            let resp = client
                .post(&url)
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("Content-Type", "application/json")
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
                return Err(format!(
                    "API error {} from provider 'anthropic' ({}) for model '{}': {}",
                    status, self.base_url, self.model_name, err_text
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
                        if let Ok(events) = adapter.parse_sse_event(&sse_event) {
                            for ev in events {
                                if tx.send(ev).await.is_err() {
                                    return;
                                }
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
