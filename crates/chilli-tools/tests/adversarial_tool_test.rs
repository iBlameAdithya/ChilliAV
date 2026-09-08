use chilli_tools::native_tools::{ExecTool, ReadFileTool, Tool, WriteFileTool};
use serde_json::json;
use tempfile::tempdir;

#[test]
fn test_adversarial_read_file_path_traversal() {
    let temp = tempdir().unwrap();
    let workspace = temp.path().canonicalize().unwrap();

    let read_tool = ReadFileTool::new(workspace.clone());

    // 1. Path traversal payload attempting to leave workspace
    let res = read_tool
        .execute(json!({ "path": "../../../Windows/System32/drivers/etc/hosts" }))
        .unwrap();

    assert!(!res.success);
    assert!(res.output.contains("Permission denied"));

    // 2. Sensitive file access payload (.ssh/id_rsa)
    let res2 = read_tool.execute(json!({ "path": ".ssh/id_rsa" })).unwrap();

    assert!(!res2.success);
    assert!(res2.output.contains("Permission denied"));
}

#[test]
fn test_adversarial_write_file_outside_workspace() {
    let temp = tempdir().unwrap();
    let workspace = temp.path().canonicalize().unwrap();

    let write_tool = WriteFileTool::new(workspace.clone());

    // Write file outside workspace root attempt
    let res = write_tool
        .execute(json!({
            "path": "../evil_script.sh",
            "content": "echo malicious"
        }))
        .unwrap();

    assert!(!res.success);
    assert!(res.output.contains("Permission denied"));
}

#[test]
fn test_adversarial_exec_command_injection() {
    let temp = tempdir().unwrap();
    let workspace = temp.path().canonicalize().unwrap();

    let exec_tool = ExecTool::new(workspace.clone());

    // Destructive rm -rf command attempt
    let res = exec_tool
        .execute(json!({
            "command": "rm -rf ."
        }))
        .unwrap();

    assert!(!res.success);
    assert!(res.output.contains("Permission denied"));
}
