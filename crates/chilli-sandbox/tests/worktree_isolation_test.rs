use chilli_sandbox::worktree::{WorktreeManager, WorktreeMode};

#[test]
fn test_direct_worktree_uses_repo_root() {
    let temp_dir = tempfile::tempdir().unwrap();
    let guard = WorktreeManager::create(temp_dir.path(), WorktreeMode::Direct).unwrap();
    assert_eq!(guard.path(), temp_dir.path());
}

#[test]
fn test_isolated_worktree_creates_and_cleans_up() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_path = temp_dir.path();

    let status = std::process::Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .output();

    if let Ok(out) = status {
        if out.status.success() {
            let _ = std::process::Command::new("git")
                .args(["config", "user.name", "test"])
                .current_dir(repo_path)
                .output();
            let _ = std::process::Command::new("git")
                .args(["config", "user.email", "test@test.com"])
                .current_dir(repo_path)
                .output();
            std::fs::write(repo_path.join("README.md"), "# Test").unwrap();
            let _ = std::process::Command::new("git")
                .args(["add", "."])
                .current_dir(repo_path)
                .output();
            let _ = std::process::Command::new("git")
                .args(["commit", "-m", "initial"])
                .current_dir(repo_path)
                .output();

            let wt_path;
            {
                let guard = WorktreeManager::create(repo_path, WorktreeMode::Isolated).unwrap();
                wt_path = guard.path().to_path_buf();
                assert!(wt_path.exists());
                assert!(wt_path.starts_with(repo_path.join(".chilli").join("worktrees")));
            }
            assert!(!wt_path.exists());
        }
    }
}
