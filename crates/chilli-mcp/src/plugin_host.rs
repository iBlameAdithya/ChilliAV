//! plugin_host.rs: MCP & Plugin Host managing external plugins (GitHub, DB, Docs, Browser, Issue Trackers, CI/CD) under capability and permission boundaries.

use crate::client::{McpClient, McpClientError};
use crate::tool_bridge::{McpToolDefinition, McpToolRegistry};
use chilli_policy::path_policy::PathPolicy;
use chilli_tools::native_tools::ToolResult;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginCategory {
    Github,
    Database,
    Documentation,
    Browser,
    IssueTracker,
    CiCd,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CapabilityBoundary {
    ReadWorkspace,
    WriteWorkspace,
    NetworkAccess,
    ShellExecution,
    CredentialAccess,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    pub name: String,
    pub category: PluginCategory,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub allowed_capabilities: HashSet<CapabilityBoundary>,
}

impl PluginConfig {
    pub fn new(name: &str, category: PluginCategory, command: &str) -> Self {
        Self {
            name: name.to_string(),
            category,
            command: command.to_string(),
            args: Vec::new(),
            env: HashMap::new(),
            allowed_capabilities: HashSet::new(),
        }
    }

    pub fn with_capability(mut self, cap: CapabilityBoundary) -> Self {
        self.allowed_capabilities.insert(cap);
        self
    }

    pub fn with_args(mut self, args: &[String]) -> Self {
        self.args = args.to_vec();
        self
    }
}

#[derive(Error, Debug)]
pub enum PluginHostError {
    #[error("MCP client error: {0}")]
    Client(#[from] McpClientError),
    #[error("Capability permission denied: {0}")]
    PermissionDenied(String),
    #[error("Plugin not found: {0}")]
    PluginNotFound(String),
    #[error("Invalid plugin manifest: {0}")]
    InvalidManifest(String),
}

pub struct McpPluginHost {
    workspace_root: PathBuf,
    policy: PathPolicy,
    plugins: HashMap<String, PluginConfig>,
    clients: HashMap<String, Arc<McpClient>>,
    registry: McpToolRegistry,
}

impl McpPluginHost {
    pub fn new(workspace_root: PathBuf, policy: PathPolicy) -> Self {
        let registry = McpToolRegistry::with_policy(workspace_root.clone(), policy.clone());
        Self {
            workspace_root,
            policy,
            plugins: HashMap::new(),
            clients: HashMap::new(),
            registry,
        }
    }

    pub fn workspace_root(&self) -> &PathBuf {
        &self.workspace_root
    }

    pub fn policy(&self) -> &PathPolicy {
        &self.policy
    }

    pub async fn register_plugin(
        &mut self,
        config: PluginConfig,
        client: Arc<McpClient>,
    ) -> Result<usize, PluginHostError> {
        let plugin_name = config.name.clone();
        let count = self
            .registry
            .register_server_tools(&plugin_name, client.clone())
            .await?;

        self.plugins.insert(plugin_name.clone(), config);
        self.clients.insert(plugin_name, client);

        Ok(count)
    }

    pub fn get_plugin_config(&self, name: &str) -> Option<&PluginConfig> {
        self.plugins.get(name)
    }

    pub fn has_capability(&self, plugin_name: &str, capability: &CapabilityBoundary) -> bool {
        if let Some(config) = self.plugins.get(plugin_name) {
            config.allowed_capabilities.contains(capability)
        } else {
            false
        }
    }

    pub fn list_plugins(&self) -> Vec<&PluginConfig> {
        self.plugins.values().collect()
    }

    pub fn list_plugins_by_category(&self, category: &PluginCategory) -> Vec<&PluginConfig> {
        self.plugins
            .values()
            .filter(|p| &p.category == category)
            .collect()
    }

    pub fn list_tools(&self) -> Vec<McpToolDefinition> {
        self.registry.list_tools()
    }

    pub async fn execute_tool(
        &self,
        plugin_name: &str,
        tool_name: &str,
        arguments: Option<serde_json::Value>,
        required_capability: Option<CapabilityBoundary>,
    ) -> Result<ToolResult, PluginHostError> {
        let config = self
            .plugins
            .get(plugin_name)
            .ok_or_else(|| PluginHostError::PluginNotFound(plugin_name.to_string()))?;

        if let Some(ref cap) = required_capability {
            if !config.allowed_capabilities.contains(cap) {
                return Err(PluginHostError::PermissionDenied(format!(
                    "Plugin '{}' missing required capability '{:?}'",
                    plugin_name, cap
                )));
            }
        }

        let res = self.registry.execute_tool(tool_name, arguments).await?;
        Ok(res)
    }
}
