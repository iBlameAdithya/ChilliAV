use chilli_policy::path_policy::{strip_unc_prefix, PathPolicy, PolicyDecision};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

#[test]
fn test_path_policy_sensitive_paths_and_traversal() {
    let temp_workspace = tempdir().unwrap();
    let workspace_root = temp_workspace.path().canonicalize().unwrap();

    let policy = PathPolicy;

    // 1. Valid workspace read
    let valid_file = workspace_root.join("src/lib.rs");
    fs::create_dir_all(workspace_root.join("src")).unwrap();
    fs::write(&valid_file, "pub fn hello() {}").unwrap();

    assert_eq!(
        policy.check_read(&workspace_root, &valid_file),
        PolicyDecision::Allow
    );

    // 2. Sensitive path read attempt (.ssh)
    let ssh_dir = workspace_root.join(".ssh/id_rsa");
    assert_eq!(
        policy.check_read(&workspace_root, &ssh_dir),
        PolicyDecision::Deny("Sensitive path access forbidden".to_string())
    );

    // 3. Traversal attempt out of workspace
    let outside_path = workspace_root.join("../outside_secret.txt");
    assert_eq!(
        policy.check_read(&workspace_root, &outside_path),
        PolicyDecision::Deny("Path outside workspace containment".to_string())
    );

    // 4. Non-existent file with parent canonicalization check
    let non_existent_inside = workspace_root.join("src/new_file.rs");
    assert_eq!(
        policy.check_write(&workspace_root, &non_existent_inside),
        PolicyDecision::Allow
    );

    let non_existent_outside_traversal = workspace_root.join("src/../../escaped.rs");
    assert_eq!(
        policy.check_write(&workspace_root, &non_existent_outside_traversal),
        PolicyDecision::Deny("Path outside workspace containment".to_string())
    );

    // 5. Relative path from workspace root
    let rel_path = Path::new("src/lib.rs");
    assert_eq!(
        policy.check_read(&workspace_root, rel_path),
        PolicyDecision::Allow
    );

    // 6. UNC prefix stripping test
    let unc_path = PathBuf::from(format!(r"\\?\{}", valid_file.display()));
    let stripped = strip_unc_prefix(&unc_path);
    assert!(!stripped.to_string_lossy().starts_with(r"\\?\"));

    // 7. Command sensitive pattern enforcement test
    assert!(matches!(
        policy.check_command(&workspace_root, &workspace_root, "cat .env"),
        PolicyDecision::Deny(_)
    ));
    assert!(matches!(
        policy.check_command(
            &workspace_root,
            &workspace_root,
            "git config --file .git/config"
        ),
        PolicyDecision::Deny(_)
    ));
    assert!(matches!(
        policy.check_command(&workspace_root, &workspace_root, "rm -rf /tmp"),
        PolicyDecision::Deny(_)
    ));
    assert_eq!(
        policy.check_command(&workspace_root, &workspace_root, "cargo test"),
        PolicyDecision::Allow
    );
}

#[test]
fn test_symlink_escape_containment() {
    let temp_workspace = tempdir().unwrap();
    let temp_outside = tempdir().unwrap();

    let workspace_root = temp_workspace.path().canonicalize().unwrap();
    let outside_root = temp_outside.path().canonicalize().unwrap();

    let outside_file = outside_root.join("secret_outside.txt");
    fs::write(&outside_file, "secret payload").unwrap();

    let policy = PathPolicy;

    // Create a symlink inside workspace pointing to outside_file
    let symlink_path = workspace_root.join("symlink_outside.txt");

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        if symlink(&outside_file, &symlink_path).is_ok() {
            assert_eq!(
                policy.check_read(&workspace_root, &symlink_path),
                PolicyDecision::Deny("Path outside workspace containment".to_string())
            );
        }
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::symlink_file;
        if symlink_file(&outside_file, &symlink_path).is_ok() {
            assert_eq!(
                policy.check_read(&workspace_root, &symlink_path),
                PolicyDecision::Deny("Path outside workspace containment".to_string())
            );
        }
    }
}
