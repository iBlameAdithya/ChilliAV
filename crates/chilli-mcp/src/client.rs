//! client.rs: Stdio process-isolated MCP client transport.

use crate::protocol::*;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

#[derive(Error, Debug)]
pub enum McpClientError {
    #[error("Failed to spawn process: {0}")]
    ProcessSpawn(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON-RPC error [{code}]: {message}")]
    JsonRpc { code: i64, message: String },
    #[error("Protocol error: {0}")]
    Protocol(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub struct McpClient {
    server_name: String,
    request_id: AtomicU64,
    stdin: Arc<Mutex<ChildStdin>>,
    stdout_reader: Arc<Mutex<BufReader<ChildStdout>>>,
    _child: Child,
}

impl McpClient {
    pub async fn spawn(
        server_name: &str,
        command: &str,
        args: &[String],
        env: Option<&HashMap<String, String>>,
    ) -> Result<Self, McpClientError> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        if let Some(env_map) = env {
            for (k, v) in env_map {
                cmd.env(k, v);
            }
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| McpClientError::ProcessSpawn(e.to_string()))?;

        let stdin = child.stdin.take().ok_ok_or_else(|| {
            McpClientError::ProcessSpawn("Failed to capture stdin".to_string())
        })?;
        let stdout = child.stdout.take().ok_ok_or_else(|| {
            McpClientError::ProcessSpawn("Failed to capture stdout".to_string())
        })?;

        let client = Self {
            server_name: server_name.to_string(),
            request_id: AtomicU64::new(1),
            stdin: Arc::new(Mutex::new(stdin)),
            stdout_reader: Arc::new(Mutex::new(BufReader::new(stdout))),
            _child: child,
        };

        client.initialize().await?;

        Ok(client)
    }

    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    async fn next_id(&self) -> u64 {
        self.request_id.fetch_add(1, Ordering::SeqCst)
    }

    async fn send_request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, McpClientError> {
        let id = self.next_id().await;
        let req = JsonRpcRequest::new(id, method, params);
        let mut req_str = serde_json::to_string(&req)?;
        req_str.push('\n');

        {
            let mut stdin = self.stdin.lock().await;
            stdin.write_all(req_str.as_bytes()).await?;
            stdin.flush().await?;
        }

        let line = {
            let mut reader = self.stdout_reader.lock().await;
            let mut line = String::new();
            let n = reader.read_line(&mut line).await?;
            if n == 0 {
                return Err(McpClientError::Protocol(
                    "Unexpected EOF reading MCP server stdout".to_string(),
                ));
            }
            line
        };

        let response: JsonRpcResponse = serde_json::from_str(&line)?;
        if let Some(err) = response.error {
            return Err(McpClientError::JsonRpc {
                code: err.code,
                message: err.message,
            });
        }

        response.result.ok_or_else(|| {
            McpClientError::Protocol("Missing result payload in response".to_string())
        })
    }

    async fn send_notification(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<(), McpClientError> {
        let notif = JsonRpcNotification::new(method, params);
        let mut notif_str = serde_json::to_string(&notif)?;
        notif_str.push('\n');

        let mut stdin = self.stdin.lock().await;
        stdin.write_all(notif_str.as_bytes()).await?;
        stdin.flush().await?;
        Ok(())
    }

    async fn initialize(&self) -> Result<InitializeResult, McpClientError> {
        let params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "chilli",
                "version": "0.1.0"
            }
        });

        let res_val = self.send_request("initialize", Some(params)).await?;
        let init_result: InitializeResult = serde_json::from_value(res_val)?;

        self.send_notification("notifications/initialized", None)
            .await?;

        Ok(init_result)
    }

    pub async fn list_tools(&self) -> Result<Vec<McpTool>, McpClientError> {
        let res_val = self.send_request("tools/list", None).await?;
        let tool_list: McpToolListResult = serde_json::from_value(res_val)?;
        Ok(tool_list.tools)
    }

    pub async fn call_tool(
        &self,
        name: &str,
        arguments: Option<serde_json::Value>,
    ) -> Result<McpCallToolResult, McpClientError> {
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments.unwrap_or(serde_json::json!({}))
        });

        let res_val = self.send_request("tools/call", Some(params)).await?;
        let call_result: McpCallToolResult = serde_json::from_value(res_val)?;
        Ok(call_result)
    }
}

trait OptionExt<T> {
    fn ok_ok_or_else<F: FnOnce() -> McpClientError>(self, err_fn: F) -> Result<T, McpClientError>;
}

impl<T> OptionExt<T> for Option<T> {
    fn ok_ok_or_else<F: FnOnce() -> McpClientError>(self, err_fn: F) -> Result<T, McpClientError> {
        match self {
            Some(v) => Ok(v),
            None => Err(err_fn()),
        }
    }
}
