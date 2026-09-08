use chilli_tools::native_tools::{ExecTool, Tool};
use serde_json::json;
use std::time::Instant;
use tempfile::tempdir;

#[test]
fn test_process_timeout_kill_cleanup() {
    let temp = tempdir().unwrap();
    let workspace = temp.path().canonicalize().unwrap();

    let exec_tool = ExecTool::new(workspace.clone());

    let start = Instant::now();

    #[cfg(windows)]
    let timeout_cmd = "ping 127.0.0.1 -n 10";

    #[cfg(not(windows))]
    let timeout_cmd = "sleep 10";

    let res = exec_tool
        .execute(json!({
            "command": timeout_cmd,
            "timeout_ms": 150
        }))
        .unwrap();

    let elapsed = start.elapsed();

    assert!(!res.success);
    assert!(res.output.contains("timed out"));
    assert!(
        elapsed.as_millis() < 3000,
        "Process did not kill properly on timeout! Took {}ms",
        elapsed.as_millis()
    );
}

#[test]
fn test_environment_inheritance_sanitization() {
    let temp = tempdir().unwrap();
    let workspace = temp.path().canonicalize().unwrap();

    // Set a sensitive environment variable on host
    std::env::set_var("OPENAI_API_KEY", "sk-secret-1234567890abcdef1234567890");

    let exec_tool = ExecTool::new(workspace.clone());

    #[cfg(windows)]
    let check_cmd = "cmd /c echo %OPENAI_API_KEY%";

    #[cfg(not(windows))]
    let check_cmd = "env";

    let res = exec_tool
        .execute(json!({
            "command": check_cmd
        }))
        .unwrap();

    // Environment variable should be stripped or absent in output
    assert!(
        !res.output.contains("sk-secret-1234567890abcdef1234567890"),
        "Sensitive environment variable leaked to child process! Output:\n{}",
        res.output
    );
}
