use chilli_mcp::{McpClient, McpToolRegistry};
use chilli_policy::path_policy::PathPolicy;
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn test_mcp_tool_bridge_and_registry_with_mock_server() {
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
                "serverInfo": { "name": "mock-mcp-server", "version": "1.0.0" }
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
                        "name": "read_remote_file",
                        "description": "Read file contents",
                        "inputSchema": { "type": "object" }
                    }
                ]
            }
        }
        print(json.dumps(res), flush=True)
    elif method == "tools/call":
        args = req.get("params", {}).get("arguments", {})
        path = args.get("path", "")
        res = {
            "jsonrpc": "2.0",
            "id": msg_id,
            "result": {
                "content": [
                    { "type": "text", "text": "Content of " + str(path) }
                ],
                "is_error": False
            }
        }
        print(json.dumps(res), flush=True)
"#;

    args.push(script.to_string());
    let python_cmd = if cfg!(windows) { "python" } else { "python3" };

    let client = McpClient::spawn("mock-mcp", python_cmd, &args, None)
        .await
        .unwrap();
    let client_arc = Arc::new(client);

    let temp = tempdir().unwrap();
    let workspace_root = temp.path().to_path_buf();
    let policy = PathPolicy;

    let mut registry = McpToolRegistry::with_policy(workspace_root.clone(), policy.clone());
    let registered_count = registry
        .register_server_tools("mock-mcp", client_arc.clone())
        .await
        .unwrap();

    assert_eq!(registered_count, 1);
    assert_eq!(registry.list_tools().len(), 1);

    // Test valid tool execution through registry
    let tool_res = registry
        .execute_tool(
            "read_remote_file",
            Some(serde_json::json!({ "path": "file.txt" })),
        )
        .await
        .unwrap();

    assert!(tool_res.success);
    assert_eq!(tool_res.output, "Content of file.txt");

    // Test path policy denial on forbidden sensitive path
    let denied_res = registry
        .execute_tool(
            "read_remote_file",
            Some(serde_json::json!({ "path": ".ssh/id_rsa" })),
        )
        .await
        .unwrap();

    assert!(!denied_res.success);
    assert!(denied_res.output.contains("Permission denied"));
}
