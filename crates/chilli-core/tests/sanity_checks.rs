use chilli_core::engine::AgentEngine;
use chilli_mcp::client::McpClient;
use chilli_memory::journal::{EventJournal, EventRecord};
use chilli_model::adapter::{CompletionRequest, StreamAccumulator};
use chilli_model::create_adapter_for_model;
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;
use tokio_stream::StreamExt;

#[tokio::test]
async fn test_1_llm_api_key_sanity_check() {
    println!("\n=======================================================");
    println!("=== TEST 1: LLM API Key Sanity Check (chilli-model) ===");
    println!("=======================================================");
    
    let has_openai = std::env::var("OPENAI_API_KEY").is_ok();
    let has_anthropic = std::env::var("ANTHROPIC_API_KEY").is_ok();
    let has_groq = std::env::var("GROQ_API_KEY").is_ok();
    let has_openrouter = std::env::var("OPENROUTER_API_KEY").is_ok();
    let has_agcl = std::env::var("AGCL_BASE_URL").is_ok();
    let target_key_file = chilli_model::find_target_api_key();

    println!("Detected Environment Keys:");
    println!("  OPENAI_API_KEY: {}", if has_openai { "SET" } else { "NOT SET" });
    println!("  ANTHROPIC_API_KEY: {}", if has_anthropic { "SET" } else { "NOT SET" });
    println!("  GROQ_API_KEY: {}", if has_groq { "SET" } else { "NOT SET" });
    println!("  OPENROUTER_API_KEY: {}", if has_openrouter { "SET" } else { "NOT SET" });
    println!("  AGCL_BASE_URL: {}", if has_agcl { "SET" } else { "NOT SET" });
    println!("  target/.api_key file: {}", if target_key_file.is_some() { "FOUND" } else { "NOT FOUND" });

    let adapter = create_adapter_for_model(None);
    println!("Provider Adapter Selected: {}", adapter.provider_name());
    println!("Model Selected: {}", adapter.model_name());

    let request = CompletionRequest {
        prompt: "ping".to_string(),
        max_tokens: Some(10),
        ..Default::default()
    };

    match adapter.complete(request).await {
        Ok(mut stream) => {
            let mut accumulator = StreamAccumulator::default();
            while let Some(event) = stream.next().await {
                accumulator.process(event);
            }
            let response = accumulator.into_response();
            println!("Response Received Successfully!");
            println!("Output Text: {:?}", response.text_content.trim());
            println!("Stop Reason: {:?}", response.stop_reason);
            assert!(
                response.stop_reason.is_some(),
                "Expected stop reason in LLM response"
            );
            println!("SUCCESS: LLM API Key Sanity Check PASSED!");
        }
        Err(err) => {
            println!("API Request Response/Diagnostic: {}", err);
            if !has_openai && !has_anthropic && !has_groq && !has_openrouter && target_key_file.is_none() {
                println!("NOTE: No API keys set in current environment or target/.api_key file.");
            }
        }
    }
}

#[tokio::test]
async fn test_2_stdio_mcp_subprocess_handshake() {
    println!("\n================================================================");
    println!("=== TEST 2: Stdio MCP Subprocess Handshake Test (chilli-mcp) ===");
    println!("================================================================");

    let dir = tempdir().unwrap();
    let script_path = dir.path().join("mock_mcp_server.py");

    let python_script = r#"
import sys
import json

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    try:
        req = json.loads(line)
        req_id = req.get("id")
        method = req.get("method")
        
        if method == "initialize":
            resp = {
                "jsonrpc": "2.0",
                "id": req_id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "mock-mcp-server", "version": "1.0.0"}
                }
            }
            sys.stdout.write(json.dumps(resp) + "\n")
            sys.stdout.flush()
        elif method == "notifications/initialized":
            pass
        elif method == "tools/list":
            resp = {
                "jsonrpc": "2.0",
                "id": req_id,
                "result": {
                    "tools": [
                        {
                            "name": "ping",
                            "description": "Mock ping tool",
                            "inputSchema": {"type": "object"}
                        }
                    ]
                }
            }
            sys.stdout.write(json.dumps(resp) + "\n")
            sys.stdout.flush()
        elif method == "tools/call":
            resp = {
                "jsonrpc": "2.0",
                "id": req_id,
                "result": {
                    "content": [{"type": "text", "text": "pong"}],
                    "isError": False
                }
            }
            sys.stdout.write(json.dumps(resp) + "\n")
            sys.stdout.flush()
    except Exception as e:
        sys.stderr.write(f"Error: {e}\n")
"#;

    let mut file = File::create(&script_path).unwrap();
    file.write_all(python_script.as_bytes()).unwrap();
    drop(file);

    let script_str = script_path.to_string_lossy().to_string();
    println!("Spawning stdio process: python {}", script_str);

    let client = McpClient::spawn(
        "mock_mcp",
        "python",
        &[script_str],
        None,
    )
    .await
    .expect("MCP client spawn and handshake failed");

    println!("Handshake complete! Server name: {}", client.server_name());

    let tools = client.list_tools().await.expect("Failed to list tools");
    println!("List tools response received: {} tool(s) found", tools.len());
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "ping");

    let call_res = client
        .call_tool("ping", None)
        .await
        .expect("Failed to call tool");
    println!("Tool call response received: contains {} content block(s)", call_res.content.len());
    assert!(!call_res.is_error, "Tool call should not return an error");

    println!("SUCCESS: Stdio MCP Subprocess Handshake Test PASSED!");
}

#[tokio::test]
async fn test_3_sqlite_wal_journal_file() {
    println!("\n========================================================");
    println!("=== TEST 3: SQLite WAL Journal File Test (chilli-core) ===");
    println!("========================================================");

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("journal.db");

    println!("Opening EventJournal at: {:?}", db_path);
    let journal = EventJournal::open(&db_path).expect("Failed to open journal db");

    let dummy_event = EventRecord {
        seq: 0,
        session_id: "test_demo_session".to_string(),
        event_type: "dummy_engine_event".to_string(),
        payload_json: r#"{"status":"triggered","agent":"chilli-demo"}"#.to_string(),
        timestamp_ms: 1700000000000,
    };

    let seq = journal.append(&dummy_event).expect("Failed to append dummy event");
    println!("Dummy event appended! Sequence ID returned: {}", seq);
    assert!(seq > 0, "Sequence ID should be positive");

    assert!(db_path.exists(), "journal.db file must exist on disk");
    println!("Confirmed journal.db exists on disk.");

    // Verify WAL journal mode
    let conn = rusqlite::Connection::open(&db_path).expect("Failed to open connection to test journal_mode");
    let mode: String = conn
        .query_row("PRAGMA journal_mode;", [], |row| row.get(0))
        .expect("Failed to read journal_mode pragma");
    println!("SQLite Journal Mode: {}", mode.to_uppercase());
    assert_eq!(mode.to_lowercase(), "wal", "Journal mode must be WAL for concurrent tailing");

    // Verify reading back events
    let events = journal
        .events_after("test_demo_session", 0)
        .expect("Failed to read back events");
    println!("Read back {} event(s) from journal", events.len());
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "dummy_engine_event");
    assert_eq!(events[0].payload_json, r#"{"status":"triggered","agent":"chilli-demo"}"#);

    // Test with AgentEngine
    let engine_workspace = tempdir().unwrap();
    let _engine = AgentEngine::new(engine_workspace.path()).expect("Failed to initialize AgentEngine");
    let engine_journal_path = engine_workspace.path().join(".chilli").join("journal.db");
    assert!(engine_journal_path.exists(), ".chilli/journal.db must be generated on AgentEngine init");
    println!("Confirmed AgentEngine generated .chilli/journal.db at {:?}", engine_journal_path);

    println!("SUCCESS: SQLite WAL Journal File Test PASSED!");
}
