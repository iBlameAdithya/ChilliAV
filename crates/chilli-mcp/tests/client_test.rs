use chilli_mcp::McpClient;

#[tokio::test]
async fn test_mock_mcp_client_stdio_handshake() {
    // Determine command to run mock server script
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
                        "name": "echo_tool",
                        "description": "Echoes back input",
                        "inputSchema": { "type": "object" }
                    }
                ]
            }
        }
        print(json.dumps(res), flush=True)
    elif method == "tools/call":
        args = req.get("params", {}).get("arguments", {})
        res = {
            "jsonrpc": "2.0",
            "id": msg_id,
            "result": {
                "content": [
                    { "type": "text", "text": "Echo: " + str(args) }
                ],
                "is_error": False
            }
        }
        print(json.dumps(res), flush=True)
"#;

    args.push(script.to_string());

    let python_cmd = if cfg!(windows) { "python" } else { "python3" };

    let client_res = McpClient::spawn("mock-server", python_cmd, &args, None).await;
    if let Ok(client) = client_res {
        assert_eq!(client.server_name(), "mock-server");

        let tools = client.list_tools().await.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "echo_tool");

        let res = client
            .call_tool("echo_tool", Some(serde_json::json!({ "msg": "hello" })))
            .await
            .unwrap();
        assert!(!res.is_error);
        assert_eq!(
            res.content[0].text.as_deref(),
            Some("Echo: {'msg': 'hello'}")
        );
    }
}
