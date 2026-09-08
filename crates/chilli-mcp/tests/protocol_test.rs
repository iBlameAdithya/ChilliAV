use chilli_mcp::protocol::*;

#[test]
fn test_jsonrpc_request_serialization() {
    let req = JsonRpcRequest::new(
        1,
        "initialize",
        Some(serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "chilli", "version": "0.1.0" }
        })),
    );

    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("\"jsonrpc\":\"2.0\""));
    assert!(json.contains("\"id\":1"));
    assert!(json.contains("\"method\":\"initialize\""));

    let deserialized: JsonRpcRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, req);
}

#[test]
fn test_mcp_tool_list_result_deserialization() {
    let raw = r#"{
        "tools": [
            {
                "name": "fetch_weather",
                "description": "Fetch current weather",
                "inputSchema": { "type": "object" }
            }
        ]
    }"#;

    let res: McpToolListResult = serde_json::from_str(raw).unwrap();
    assert_eq!(res.tools.len(), 1);
    assert_eq!(res.tools[0].name, "fetch_weather");
    assert_eq!(
        res.tools[0].description.as_deref(),
        Some("Fetch current weather")
    );
}
