use chilli_model::adapter::StreamEvent;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccumulatedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
    pub raw_arguments_json: String,
}

#[derive(Debug, Clone, Default)]
struct PendingToolCall {
    id: String,
    name: String,
    arguments_json_buffer: String,
}

#[derive(Debug, Clone, Default)]
pub struct StreamAccumulator {
    text_buffer: String,
    tool_calls: Vec<PendingToolCall>,
    pub token_usage: chilli_model::adapter::TokenUsage,
}

impl StreamAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::TextChunk(chunk) => {
                self.text_buffer.push_str(chunk);
            }
            StreamEvent::ToolCallChunk(chunk) => {
                if let Some(idx) = chunk.index {
                    let target = if !chunk.id.is_empty() {
                        if let Some(pos) = self.tool_calls.iter().position(|tc| tc.id == chunk.id) {
                            pos
                        } else {
                            while self.tool_calls.len() <= idx {
                                self.tool_calls.push(PendingToolCall::default());
                            }
                            idx
                        }
                    } else {
                        while self.tool_calls.len() <= idx {
                            self.tool_calls.push(PendingToolCall::default());
                        }
                        idx
                    };
                    let target_tc = &mut self.tool_calls[target];
                    if !chunk.id.is_empty() {
                        target_tc.id = chunk.id.clone();
                    }
                    if !chunk.name.is_empty() {
                        target_tc.name = chunk.name.clone();
                    }
                    target_tc
                        .arguments_json_buffer
                        .push_str(&chunk.arguments_json);
                } else if !chunk.id.is_empty() {
                    if let Some(existing) = self.tool_calls.iter_mut().find(|tc| tc.id == chunk.id)
                    {
                        if !chunk.name.is_empty() {
                            existing.name = chunk.name.clone();
                        }
                        existing
                            .arguments_json_buffer
                            .push_str(&chunk.arguments_json);
                    } else {
                        self.tool_calls.push(PendingToolCall {
                            id: chunk.id.clone(),
                            name: chunk.name.clone(),
                            arguments_json_buffer: chunk.arguments_json.clone(),
                        });
                    }
                } else if let Some(target_tc) = self
                    .tool_calls
                    .iter_mut()
                    .rev()
                    .find(|tc| !tc.name.is_empty() || !tc.id.is_empty())
                {
                    if !chunk.name.is_empty() {
                        target_tc.name = chunk.name.clone();
                    }
                    target_tc
                        .arguments_json_buffer
                        .push_str(&chunk.arguments_json);
                } else if let Some(last) = self.tool_calls.last_mut() {
                    if !chunk.name.is_empty() {
                        last.name = chunk.name.clone();
                    }
                    last.arguments_json_buffer.push_str(&chunk.arguments_json);
                } else {
                    self.tool_calls.push(PendingToolCall {
                        id: chunk.id.clone(),
                        name: chunk.name.clone(),
                        arguments_json_buffer: chunk.arguments_json.clone(),
                    });
                }
            }
            StreamEvent::Usage(usage) => {
                self.token_usage = usage.clone();
            }
            _ => {}
        }
    }

    pub fn accumulated_text(&self) -> &str {
        &self.text_buffer
    }

    pub fn finish_tool_calls(&mut self) -> Result<Vec<AccumulatedToolCall>, String> {
        let mut finished = Vec::new();
        for pending in &self.tool_calls {
            if pending.name.trim().is_empty() {
                continue;
            }
            let parsed_json: Value = if pending.arguments_json_buffer.trim().is_empty() {
                serde_json::json!({})
            } else {
                serde_json::from_str(&pending.arguments_json_buffer).map_err(|e| {
                    format!("Failed to parse tool call JSON for {}: {}", pending.name, e)
                })?
            };

            finished.push(AccumulatedToolCall {
                id: pending.id.clone(),
                name: pending.name.clone(),
                arguments: parsed_json,
                raw_arguments_json: pending.arguments_json_buffer.clone(),
            });
        }

        if finished.is_empty() {
            finished = extract_fuzzy_tool_calls_from_text(&self.text_buffer);
        }

        Ok(finished)
    }
}

pub fn extract_fuzzy_tool_calls_from_text(text: &str) -> Vec<AccumulatedToolCall> {
    let mut results = Vec::new();
    let known_tools = [
        "read_file",
        "write_file",
        "edit_file",
        "grep",
        "list_directory",
        "exec",
        "execute_command",
        "move_file",
        "copy_file",
        "create_directory",
        "remove_file",
    ];

    let parse_val_to_tool_call = |val: &Value, idx: usize| -> Option<AccumulatedToolCall> {
        let obj = val.as_object()?;

        let name_str = obj
            .get("name")
            .or_else(|| obj.get("tool"))
            .or_else(|| obj.get("tool_name"))
            .or_else(|| obj.get("function"))
            .or_else(|| obj.get("action"))
            .and_then(|v| v.as_str())?;

        let name = match name_str {
            "execute_command" | "run_command" | "bash" | "shell" => "exec",
            "read_file_content" => "read_file",
            "write_file_content" => "write_file",
            other => other,
        };

        if !known_tools.contains(&name) && name != "exec" {
            return None;
        }

        let mut args = if let Some(args_val) = obj
            .get("arguments")
            .or_else(|| obj.get("parameters"))
            .or_else(|| obj.get("args"))
            .or_else(|| obj.get("params"))
            .or_else(|| obj.get("input"))
        {
            args_val.clone()
        } else {
            let mut copy = obj.clone();
            copy.remove("name");
            copy.remove("tool");
            copy.remove("tool_name");
            copy.remove("function");
            copy.remove("action");
            Value::Object(copy)
        };

        if let Some(args_obj) = args.as_object_mut() {
            if !args_obj.contains_key("path") {
                if let Some(p) = args_obj
                    .get("file_path")
                    .or_else(|| args_obj.get("filepath"))
                    .or_else(|| args_obj.get("filename"))
                    .or_else(|| args_obj.get("file"))
                    .cloned()
                {
                    args_obj.insert("path".to_string(), p);
                }
            }
            if !args_obj.contains_key("command") {
                if let Some(c) = args_obj.get("cmd").cloned() {
                    args_obj.insert("command".to_string(), c);
                }
            }
            if !args_obj.contains_key("query") {
                if let Some(q) = args_obj
                    .get("pattern")
                    .or_else(|| args_obj.get("search_term"))
                    .cloned()
                {
                    args_obj.insert("query".to_string(), q);
                }
            }
        }

        let raw_arguments_json = serde_json::to_string(&args).unwrap_or_default();
        let id = format!("call_fuzzy_{}", idx);

        Some(AccumulatedToolCall {
            id,
            name: name.to_string(),
            arguments: args,
            raw_arguments_json,
        })
    };

    let mut search_pos = 0;
    while let Some(start_idx) = text[search_pos..].find("```") {
        let abs_start = search_pos + start_idx;
        let content_start = abs_start + 3;
        let line_end = text[content_start..]
            .find('\n')
            .map(|i| content_start + i + 1)
            .unwrap_or(content_start);

        if let Some(end_idx) = text[line_end..].find("```") {
            let block_content = &text[line_end..line_end + end_idx];
            if let Ok(val) = serde_json::from_str::<Value>(block_content.trim()) {
                if let Some(arr) = val.as_array() {
                    for item in arr {
                        if let Some(tc) = parse_val_to_tool_call(item, results.len()) {
                            results.push(tc);
                        }
                    }
                } else if let Some(tc) = parse_val_to_tool_call(&val, results.len()) {
                    results.push(tc);
                }
            }
            search_pos = line_end + end_idx + 3;
        } else {
            break;
        }
    }

    if results.is_empty() {
        let chars: Vec<char> = text.chars().collect();
        let mut bytes_idx = 0;
        while bytes_idx < chars.len() {
            if chars[bytes_idx] == '{' {
                let mut depth = 0;
                let mut end = bytes_idx;
                #[allow(clippy::needless_range_loop)]
                for i in bytes_idx..chars.len() {
                    if chars[i] == '{' {
                        depth += 1;
                    } else if chars[i] == '}' {
                        depth -= 1;
                        if depth == 0 {
                            end = i;
                            break;
                        }
                    }
                }
                if depth == 0 && end > bytes_idx {
                    let candidate: String = chars[bytes_idx..=end].iter().collect();
                    if let Ok(val) = serde_json::from_str::<Value>(&candidate) {
                        if let Some(tc) = parse_val_to_tool_call(&val, results.len()) {
                            results.push(tc);
                        }
                    }
                    bytes_idx = end;
                }
            }
            bytes_idx += 1;
        }
    }

    results
}
