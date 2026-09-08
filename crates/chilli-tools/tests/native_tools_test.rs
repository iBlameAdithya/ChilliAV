use chilli_tools::native_tools::{
    CopyFileTool, CreateDirectoryTool, EditFileTool, ExecTool, GrepTool, ListDirectoryTool,
    MoveFileTool, ReadFileTool, RemoveFileTool, Tool, WriteFileTool,
};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_read_write_edit_grep_tools() {
    let temp_workspace = tempdir().unwrap();
    let root = temp_workspace.path().canonicalize().unwrap();

    let write_tool = WriteFileTool::new(root.clone());
    let read_tool = ReadFileTool::new(root.clone());
    let edit_tool = EditFileTool::new(root.clone());
    let grep_tool = GrepTool::new(root.clone());

    // 1. Write file
    let target = root.join("hello.txt");
    let write_res = write_tool
        .execute(serde_json::json!({
            "path": "hello.txt",
            "content": "Line 1: Hello\nLine 2: World\nLine 3: Chilli"
        }))
        .unwrap();

    assert!(write_res.success);
    assert!(target.exists());

    // 2. Read file
    let read_res = read_tool
        .execute(serde_json::json!({
            "path": "hello.txt",
            "offset": 1,
            "limit": 2
        }))
        .unwrap();

    assert!(read_res.success);
    assert!(read_res.output.contains("Line 1: Hello"));
    assert!(read_res.output.contains("Line 2: World"));
    assert!(!read_res.output.contains("Line 3: Chilli"));

    // 3. Edit file (Tier 1 exact match)
    let edit_res = edit_tool
        .execute(serde_json::json!({
            "path": "hello.txt",
            "old_string": "World",
            "new_string": "Rust"
        }))
        .unwrap();

    assert!(edit_res.success);

    let updated_content = fs::read_to_string(&target).unwrap();
    assert!(updated_content.contains("Line 2: Rust"));

    // 3b. Edit file (Tier 2 normalized line endings match)
    let edit_res_t2 = edit_tool
        .execute(serde_json::json!({
            "path": "hello.txt",
            "old_string": "Line 1: Hello\r\nLine 2: Rust",
            "new_string": "Line 1: Hello\nLine 2: Rustacean"
        }))
        .unwrap();
    assert!(edit_res_t2.success);

    // 3c. Edit file (Tier 3 trimmed indentation match)
    let edit_res_t3 = edit_tool
        .execute(serde_json::json!({
            "path": "hello.txt",
            "old_string": "   Line 2: Rustacean   ",
            "new_string": "Line 2: Gopher"
        }))
        .unwrap();
    assert!(edit_res_t3.success);
    assert!(fs::read_to_string(&target)
        .unwrap()
        .contains("Line 2: Gopher"));

    // 3d. Edit file (Tier 4 diagnostic error on non-match)
    let edit_res_t4 = edit_tool
        .execute(serde_json::json!({
            "path": "hello.txt",
            "old_string": "Nonexistent text line",
            "new_string": "Something else"
        }))
        .unwrap();
    assert!(!edit_res_t4.success);
    assert!(edit_res_t4
        .output
        .contains("Tier 1-3 match strategies failed"));

    // 4. Grep file
    let grep_res = grep_tool
        .execute(serde_json::json!({
            "pattern": "Gopher"
        }))
        .unwrap();

    assert!(grep_res.success);
    assert!(grep_res.output.contains("hello.txt:2: Line 2: Gopher"));

    // 5. Containment failure attempt
    let bad_read = read_tool
        .execute(serde_json::json!({
            "path": "../secret.txt"
        }))
        .unwrap();

    assert!(!bad_read.success);
    assert!(
        bad_read
            .output
            .contains("Path outside workspace containment")
            || bad_read.output.contains("forbidden")
    );

    // 6. List Directory Tool
    let list_dir_tool = ListDirectoryTool::new(root.clone());
    let list_res = list_dir_tool
        .execute(serde_json::json!({
            "path": "."
        }))
        .unwrap();

    assert!(list_res.success);
    assert!(list_res.output.contains("[FILE] hello.txt"));

    // 6b. List Directory Tool on a file returns diagnostic feedback
    let list_file_res = list_dir_tool
        .execute(serde_json::json!({
            "path": "hello.txt"
        }))
        .unwrap();

    assert!(!list_file_res.success);
    assert!(list_file_res.output.contains(
        "Target 'hello.txt' is a file, not a directory. Use 'read_file' to view file contents."
    ));
}

#[test]
fn test_native_fs_operations() {
    let temp_workspace = tempdir().unwrap();
    let root = temp_workspace.path().canonicalize().unwrap();

    let write_tool = WriteFileTool::new(root.clone());
    let copy_tool = CopyFileTool::new(root.clone());
    let move_tool = MoveFileTool::new(root.clone());
    let create_dir_tool = CreateDirectoryTool::new(root.clone());
    let remove_tool = RemoveFileTool::new(root.clone());

    // 1. Create directory
    let create_res = create_dir_tool
        .execute(serde_json::json!({ "path": "sub/folder" }))
        .unwrap();
    assert!(create_res.success);
    assert!(root.join("sub/folder").is_dir());

    // 2. Write file
    write_tool
        .execute(serde_json::json!({
            "path": "sub/folder/file.txt",
            "content": "test content"
        }))
        .unwrap();

    // 3. Copy file
    let copy_res = copy_tool
        .execute(serde_json::json!({
            "from": "sub/folder/file.txt",
            "to": "sub/folder/copied.txt"
        }))
        .unwrap();
    assert!(copy_res.success);
    assert!(root.join("sub/folder/copied.txt").exists());

    // 4. Move file
    let move_res = move_tool
        .execute(serde_json::json!({
            "from": "sub/folder/copied.txt",
            "to": "sub/moved.txt"
        }))
        .unwrap();
    assert!(move_res.success);
    assert!(!root.join("sub/folder/copied.txt").exists());
    assert!(root.join("sub/moved.txt").exists());

    // 5. Remove file
    let remove_res = remove_tool
        .execute(serde_json::json!({ "path": "sub/moved.txt" }))
        .unwrap();
    assert!(remove_res.success);
    assert!(!root.join("sub/moved.txt").exists());

    // 6. Containment policy check on remove
    let bad_remove = remove_tool
        .execute(serde_json::json!({ "path": "../outside.txt" }))
        .unwrap();
    assert!(!bad_remove.success);
    assert!(bad_remove
        .output
        .contains("Path outside workspace containment"));
}

#[test]
fn test_create_directory_file_guard() {
    let temp_workspace = tempdir().unwrap();
    let root = temp_workspace.path().canonicalize().unwrap();

    let write_tool = WriteFileTool::new(root.clone());
    let create_dir_tool = CreateDirectoryTool::new(root.clone());

    // 1. Existing file check
    write_tool
        .execute(serde_json::json!({
            "path": "existing_file.rs",
            "content": "pub fn main() {}"
        }))
        .unwrap();

    let res_existing = create_dir_tool
        .execute(serde_json::json!({ "path": "existing_file.rs" }))
        .unwrap();
    assert!(!res_existing.success);
    assert!(res_existing.output.contains("PATH_TYPE_ERROR"));
    assert!(res_existing.output.contains("exists as a file"));

    // 2. File extension heuristic check
    let res_ext = create_dir_tool
        .execute(serde_json::json!({ "path": "src/processor.rs" }))
        .unwrap();
    assert!(!res_ext.success);
    assert!(res_ext.output.contains("PATH_TYPE_ERROR"));
    assert!(res_ext.output.contains("has file extension '.rs'"));
}

#[test]
fn test_edit_file_tier4_diagnostics() {
    let temp_workspace = tempdir().unwrap();
    let root = temp_workspace.path().canonicalize().unwrap();

    let write_tool = WriteFileTool::new(root.clone());
    let edit_tool = EditFileTool::new(root.clone());

    write_tool
        .execute(serde_json::json!({
            "path": "test.py",
            "content": "def add(a, b):\n    return a + b\n"
        }))
        .unwrap();

    let edit_res = edit_tool
        .execute(serde_json::json!({
            "path": "test.py",
            "old_string": "def add(x, y):",
            "new_string": "def add(a: int, b: int):"
        }))
        .unwrap();

    assert!(!edit_res.success);
    assert!(edit_res.output.contains("EDIT_FAILED"));
    assert!(edit_res.output.contains("=== REQUESTED OLD_STRING ==="));
    assert!(edit_res.output.contains("RECOVERY INSTRUCTION"));
}

#[test]
fn test_exec_windows_single_quote_guard() {
    let temp_workspace = tempdir().unwrap();
    let root = temp_workspace.path().canonicalize().unwrap();

    let exec_tool = ExecTool::new(root);

    let res = exec_tool
        .execute(serde_json::json!({
            "command": "python -c 'print(\"hello\")'"
        }))
        .unwrap();

    if cfg!(windows) {
        assert!(!res.success);
        assert!(res.output.contains("COMMAND_REJECTED"));
        assert!(res
            .output
            .contains("Windows cmd.exe shell does not support single quotes"));
    }
}

#[test]
fn test_edit_file_fallback_to_write_file_on_missing_target() {
    let temp_workspace = tempdir().unwrap();
    let root = temp_workspace.path().canonicalize().unwrap();

    let edit_tool = EditFileTool::new(root.clone());

    // 1. Missing target file -> auto-converts to file creation via WriteFileTool
    let res = edit_tool
        .execute(serde_json::json!({
            "path": "src/new_file.rs",
            "old_string": "",
            "new_string": "pub fn hello() {}"
        }))
        .unwrap();

    assert!(res.success);
    assert!(root.join("src/new_file.rs").exists());
    assert_eq!(
        fs::read_to_string(root.join("src/new_file.rs")).unwrap(),
        "pub fn hello() {}"
    );
}
