use chilli_core::engine::AgentEngine;
use serde_json::json;
use tempfile::tempdir;

#[tokio::test]
async fn test_ast_syntax_warning_on_invalid_file_write() {
    let dir = tempdir().unwrap();
    let engine = AgentEngine::new(dir.path()).unwrap();

    let invalid_rust_code = "fn main() { let x = ; }";
    let args = json!({
        "path": "src/main.rs",
        "content": invalid_rust_code,
    });

    let res = engine.execute_tool("write_file", args).unwrap();
    assert!(res.success);
    assert!(res.output.contains("[SYNTAX DIAGNOSTIC WARNING"));
    assert!(res.output.contains("src/main.rs"));
}

#[tokio::test]
async fn test_ast_syntax_clean_on_valid_file_write() {
    let dir = tempdir().unwrap();
    let engine = AgentEngine::new(dir.path()).unwrap();

    let valid_rust_code = "fn main() { let x = 42; }";
    let args = json!({
        "path": "src/main.rs",
        "content": valid_rust_code,
    });

    let res = engine.execute_tool("write_file", args).unwrap();
    assert!(res.success);
    assert!(!res.output.contains("[SYNTAX DIAGNOSTIC WARNING"));
}
