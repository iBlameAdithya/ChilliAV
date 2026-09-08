//! tool_bridge.rs: Adapts MCP tools to Chilli tool execution interface and policy validation.

use crate::client::{McpClient, McpClientError};
use crate::protocol::McpTool;
use chilli_policy::path_policy::{PathPolicy, PolicyDecision};
use chilli_tools::native_tools::ToolResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDefinition {
    pub server_name: String,
    pub name: String,
    pub description: String,
    pub schema: serde_json::Value,
}

pub struct McpToolAdapter {
    definition: McpToolDefinition,
    client: Arc<McpClient>,
    workspace_root: Option<PathBuf>,
    policy: PathPolicy,
}

impl McpToolAdapter {
    pub fn new(server_name: &str, tool: McpTool, client: Arc<McpClient>) -> Self {
        Self {
            definition: McpToolDefinition {
                server_name: server_name.to_string(),
                name: tool.name,
                description: tool.description.unwrap_or_default(),
                schema: tool.input_schema,
            },
            client,
            workspace_root: None,
            policy: PathPolicy,
        }
    }

    pub fn with_policy(mut self, workspace_root: PathBuf, policy: PathPolicy) -> Self {
        self.workspace_root = Some(workspace_root);
        self.policy = policy;
        self
    }

    pub fn definition(&self) -> &McpToolDefinition {
        &self.definition
    }

    pub async fn execute(
        &self,
        arguments: Option<serde_json::Value>,
    ) -> Result<ToolResult, McpClientError> {
        // Enforce policy checks if workspace_root and arguments are configured
        if let (Some(root), Some(args)) = (&self.workspace_root, arguments.as_ref()) {
            if let Some(path_str) = args
                .get("path")
                .or_else(|| args.get("file"))
                .or_else(|| args.get("target"))
                .and_then(|v| v.as_str())
            {
                let target_path = root.join(path_str);
                let is_write = args.get("content").is_some() || args.get("new_string").is_some();

                let decision = if is_write {
                    self.policy.check_write(root, &target_path)
                } else {
                    self.policy.check_read(root, &target_path)
                };

                if let PolicyDecision::Deny(reason) = decision {
                    return Ok(ToolResult {
                        success: false,
                        output: format!("Permission denied: {}", reason),
                    });
                }
            }

            if let Some(cmd_str) = args.get("command").and_then(|v| v.as_str()) {
                let cwd = args
                    .get("cwd")
                    .and_then(|v| v.as_str())
                    .map(|s| root.join(s))
                    .unwrap_or_else(|| root.clone());

                if let PolicyDecision::Deny(reason) = self.policy.check_command(root, &cwd, cmd_str)
                {
                    return Ok(ToolResult {
                        success: false,
                        output: format!("Permission denied: {}", reason),
                    });
                }
            }
        }

        let result = self
            .client
            .call_tool(&self.definition.name, arguments)
            .await?;
        if result.is_error {
            let err_msg = result
                .content
                .iter()
                .filter_map(|c| c.text.as_deref())
                .collect::<Vec<_>>()
                .join("\n");
            return Ok(ToolResult {
                success: false,
                output: format!("MCP tool execution error: {}", err_msg),
            });
        }

        let output = result
            .content
            .iter()
            .filter_map(|c| c.text.as_deref())
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ToolResult {
            success: true,
            output,
        })
    }
}

/// Registry managing multiple MCP tools bridged across external servers.
#[derive(Default)]
pub struct McpToolRegistry {
    tools: HashMap<String, Arc<McpToolAdapter>>,
    workspace_root: Option<PathBuf>,
    policy: PathPolicy,
}

impl McpToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_policy(workspace_root: PathBuf, policy: PathPolicy) -> Self {
        Self {
            tools: HashMap::new(),
            workspace_root: Some(workspace_root),
            policy,
        }
    }

    pub async fn register_server_tools(
        &mut self,
        server_name: &str,
        client: Arc<McpClient>,
    ) -> Result<usize, McpClientError> {
        let mcp_tools = client.list_tools().await?;
        let count = mcp_tools.len();

        for tool in mcp_tools {
            let mut adapter = McpToolAdapter::new(server_name, tool, client.clone());
            if let Some(root) = &self.workspace_root {
                adapter = adapter.with_policy(root.clone(), self.policy.clone());
            }
            let name = adapter.definition().name.clone();
            self.tools.insert(name, Arc::new(adapter));
        }

        Ok(count)
    }

    pub fn get_tool(&self, name: &str) -> Option<Arc<McpToolAdapter>> {
        self.tools.get(name).cloned()
    }

    pub fn list_tools(&self) -> Vec<McpToolDefinition> {
        self.tools
            .values()
            .map(|t| t.definition().clone())
            .collect()
    }

    pub async fn execute_tool(
        &self,
        name: &str,
        arguments: Option<serde_json::Value>,
    ) -> Result<ToolResult, McpClientError> {
        let tool = self.tools.get(name).ok_or_else(|| {
            McpClientError::Protocol(format!("Tool '{}' not found in registry", name))
        })?;

        tool.execute(arguments).await
    }
}
