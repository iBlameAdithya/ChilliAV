use chilli_mcp::{CapabilityBoundary, McpClient, McpPluginHost, PluginCategory, PluginConfig};
use chilli_policy::path_policy::PathPolicy;
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn test_plugin_host_registration_capabilities_and_execution() {
    let mut args = vec!["-c".to_string()];
    let script = r#"
import sys, json

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    req = json.loads(line)
    method = req.get("method")
    msg_id = req.get("id")

    if method == "initialize":
        res = {
            "jsonrpc": "2.0",
            "id": msg_id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "serverInfo": { "name": "github-mcp", "version": "1.0.0" }
            }
        }
        print(json.dumps(res), flush=True)
    elif method == "notifications/initialized":
        continue
    elif method == "tools/list":
        res = {
            "jsonrpc": "2.0",
            "id": msg_id,
            "result": {
                "tools": [
                    {
                        "name": "create_issue",
                        "description": "Create GitHub Issue",
                        "inputSchema": { "type": "object" }
                    }
                ]
            }
        }
        print(json.dumps(res), flush=True)
    elif method == "tools/call":
        args = req.get("params", {}).get("arguments", {})
        title = args.get("title", "")
        res = {
            "jsonrpc": "2.0",
            "id": msg_id,
            "result": {
                "content": [
                    { "type": "text", "text": "Issue created: " + str(title) }
                ],
                "is_error": False
            }
        }
        print(json.dumps(res), flush=True)
"#;

    args.push(script.to_string());
    let python_cmd = if cfg!(windows) { "python" } else { "python3" };

    let client = McpClient::spawn("github-plugin", python_cmd, &args, None)
        .await
        .unwrap();
    let client_arc = Arc::new(client);

    let temp = tempdir().unwrap();
    let workspace_root = temp.path().to_path_buf();
    let policy = PathPolicy;

    let mut host = McpPluginHost::new(workspace_root, policy);

    let config = PluginConfig::new("github-plugin", PluginCategory::Github, python_cmd)
        .with_capability(CapabilityBoundary::NetworkAccess);

    let registered_count = host.register_plugin(config, client_arc).await.unwrap();
    assert_eq!(registered_count, 1);
    assert_eq!(host.list_tools().len(), 1);

    // Verify category listing
    let gh_plugins = host.list_plugins_by_category(&PluginCategory::Github);
    assert_eq!(gh_plugins.len(), 1);
    assert_eq!(gh_plugins[0].name, "github-plugin");

    // Verify capability check
    assert!(host.has_capability("github-plugin", &CapabilityBoundary::NetworkAccess));
    assert!(!host.has_capability("github-plugin", &CapabilityBoundary::ShellExecution));

    // Execute tool with granted capability
    let tool_res = host
        .execute_tool(
            "github-plugin",
            "create_issue",
            Some(serde_json::json!({ "title": "Bug in main loop" })),
            Some(CapabilityBoundary::NetworkAccess),
        )
        .await
        .unwrap();

    assert!(tool_res.success);
    assert_eq!(tool_res.output, "Issue created: Bug in main loop");

    // Attempt tool execution with ungranted capability requirement (ShellExecution)
    let denied_err = host
        .execute_tool(
            "github-plugin",
            "create_issue",
            Some(serde_json::json!({ "title": "Test" })),
            Some(CapabilityBoundary::ShellExecution),
        )
        .await;

    assert!(denied_err.is_err());
    let err_msg = denied_err.unwrap_err().to_string();
    assert!(err_msg.contains("missing required capability"));
}
