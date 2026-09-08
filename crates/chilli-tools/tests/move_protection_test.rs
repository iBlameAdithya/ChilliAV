use chilli_tools::native_tools::{MoveFileTool, Tool};
use std::fs;
use std::sync::Arc;
use tempfile::tempdir;

#[test]
fn test_move_file_destructive_guard_blocks_when_references_exist() {
    let dir = tempdir().unwrap();
    let src_path = dir.path().join("src/mod.rs");
    let dst_path = dir.path().join("src/renamed.rs");
    fs::create_dir_all(src_path.parent().unwrap()).unwrap();
    fs::write(&src_path, "pub fn foo() {}").unwrap();

    let ref_checker = Arc::new(|path: &str| -> usize {
        if path == "src/mod.rs" {
            3
        } else {
            0
        }
    });

    let tool = MoveFileTool::new(dir.path().to_path_buf()).with_ref_checker(ref_checker);

    // 1. Direct move blocked when refs > 0 and no flags
    let res = tool
        .execute(serde_json::json!({
            "from": "src/mod.rs",
            "to": "src/renamed.rs"
        }))
        .unwrap();

    assert!(!res.success);
    assert!(res.output.contains("[DESTRUCTIVE MOVE GUARD]"));
    assert!(src_path.exists());
    assert!(!dst_path.exists());

    // 2. Direct move allowed when force=true
    let res_force = tool
        .execute(serde_json::json!({
            "from": "src/mod.rs",
            "to": "src/renamed.rs",
            "force": true
        }))
        .unwrap();

    assert!(res_force.success);
    assert!(!src_path.exists());
    assert!(dst_path.exists());
}

#[test]
fn test_move_file_copy_first_preserves_source() {
    let dir = tempdir().unwrap();
    let src_path = dir.path().join("src/lib.rs");
    let dst_path = dir.path().join("src/lib_backup.rs");
    fs::create_dir_all(src_path.parent().unwrap()).unwrap();
    fs::write(&src_path, "pub fn bar() {}").unwrap();

    let ref_checker = Arc::new(|_path: &str| -> usize { 5 });

    let tool = MoveFileTool::new(dir.path().to_path_buf()).with_ref_checker(ref_checker);

    let res = tool
        .execute(serde_json::json!({
            "from": "src/lib.rs",
            "to": "src/lib_backup.rs",
            "copy_first": true
        }))
        .unwrap();

    assert!(res.success);
    assert!(res.output.contains("copy-first mode enabled"));
    assert!(src_path.exists());
    assert!(dst_path.exists());
}
