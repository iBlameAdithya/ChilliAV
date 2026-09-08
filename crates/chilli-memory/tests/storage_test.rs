use chilli_memory::storage::StoragePaths;
use tempfile::tempdir;

#[test]
fn test_storage_paths_resolve() {
    let temp_workspace = tempdir().unwrap();
    let workspace_path = temp_workspace.path();

    let paths = StoragePaths::resolve(workspace_path).unwrap();

    assert_eq!(paths.workspace_root, workspace_path);
    assert_eq!(paths.local_chilli_dir, workspace_path.join(".chilli"));
    assert_eq!(
        paths.journal_db_path,
        workspace_path.join(".chilli/journal.db")
    );
    assert_eq!(
        paths.codegraph_db_path,
        workspace_path.join(".chilli/codegraph.db")
    );
    assert_eq!(paths.cache_dir, workspace_path.join(".chilli/cache"));

    assert!(paths.local_chilli_dir.exists());
    assert!(paths.cache_dir.exists());

    let gitignore_path = workspace_path.join(".chilli/.gitignore");
    assert!(gitignore_path.exists());
    let gitignore_content = std::fs::read_to_string(gitignore_path).unwrap();
    assert_eq!(gitignore_content, "*\n");
}
