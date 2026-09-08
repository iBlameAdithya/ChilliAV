use chilli_tools::native_tools::{ExecTool, Tool};
use serde_json::json;

#[tokio::test]
async fn test_exec_tool_success_and_policy_denial() {
    let ws = std::env::temp_dir().join("chilli_exec_tool_test");
    let _ = std::fs::remove_dir_all(&ws);
    std::fs::create_dir_all(&ws).unwrap();

    let tool = ExecTool::new(ws.clone());
    let args = json!({
        "command": "cargo",
        "args": ["--version"],
        "cwd": "."
    });

    let res = tool.execute(args).unwrap();
    assert!(res.success);
    assert!(res.output.contains("cargo"));
}

#[tokio::test]
async fn test_exec_tool_cwd_outside_workspace_denied() {
    let ws = std::env::temp_dir().join("chilli_exec_tool_cwd_test");
    let _ = std::fs::remove_dir_all(&ws);
    std::fs::create_dir_all(&ws).unwrap();

    let tool = ExecTool::new(ws.clone());
    let args = json!({
        "command": "cargo",
        "args": ["--version"],
        "cwd": ".."
    });

    let res = tool.execute(args).unwrap();
    assert!(!res.success);
    assert!(res.output.contains("Permission denied"));
}

#[tokio::test]
async fn test_exec_tool_sensitive_pattern_denied() {
    let ws = std::env::temp_dir().join("chilli_exec_tool_sensitive_test");
    let _ = std::fs::remove_dir_all(&ws);
    std::fs::create_dir_all(&ws).unwrap();

    let tool = ExecTool::new(ws.clone());
    let args = json!({
        "command": "cat",
        "args": [".ssh/id_rsa"],
        "cwd": "."
    });

    let res = tool.execute(args).unwrap();
    assert!(!res.success);
    assert!(res.output.contains("Permission denied"));
}
