use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorktreeMode {
    Direct,
    Isolated,
}

pub struct WorktreeGuard {
    repo_root: PathBuf,
    worktree_path: Option<PathBuf>,
    worktree_id: Option<String>,
}

impl WorktreeGuard {
    pub fn path(&self) -> &Path {
        if let Some(ref wt) = self.worktree_path {
            wt
        } else {
            &self.repo_root
        }
    }

    pub fn is_isolated(&self) -> bool {
        self.worktree_path.is_some()
    }

    pub fn sync_modified_files_to_main(&self) -> Result<Vec<String>, String> {
        let wt_path = match self.worktree_path {
            Some(ref p) => p,
            None => return Ok(Vec::new()),
        };

        let mut synced_files = Vec::new();
        Self::copy_dir_recursive(wt_path, wt_path, &self.repo_root, &mut synced_files)?;
        Ok(synced_files)
    }

    fn copy_dir_recursive(
        current_dir: &Path,
        wt_base: &Path,
        repo_root: &Path,
        synced_files: &mut Vec<String>,
    ) -> Result<(), String> {
        let entries = match fs::read_dir(current_dir) {
            Ok(iter) => iter,
            Err(_) => return Ok(()),
        };

        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let file_name = path.file_name().unwrap_or_default().to_string_lossy();

            if file_name == ".git" || file_name == "target" || file_name == ".chilli" {
                continue;
            }

            let rel_path = path
                .strip_prefix(wt_base)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");

            if path.is_dir() {
                Self::copy_dir_recursive(&path, wt_base, repo_root, synced_files)?;
            } else if path.is_file() {
                let target_path = repo_root.join(&rel_path);
                if let Some(parent) = target_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                fs::copy(&path, &target_path).map_err(|e| e.to_string())?;
                synced_files.push(rel_path);
            }
        }
        Ok(())
    }
}

impl Drop for WorktreeGuard {
    fn drop(&mut self) {
        if let (Some(ref wt_path), Some(ref id)) = (&self.worktree_path, &self.worktree_id) {
            let status = Command::new("git")
                .args(["worktree", "remove", "--force", &wt_path.to_string_lossy()])
                .current_dir(&self.repo_root)
                .output();

            if status.is_err() || !status.as_ref().unwrap().status.success() {
                let _ = fs::remove_dir_all(wt_path);
            }

            let branch_name = format!("chilli-wt-{}", id);
            let _ = Command::new("git")
                .args(["branch", "-D", &branch_name])
                .current_dir(&self.repo_root)
                .output();
        }
    }
}

pub struct WorktreeManager;

impl WorktreeManager {
    pub fn create(repo_root: &Path, mode: WorktreeMode) -> Result<WorktreeGuard, String> {
        match mode {
            WorktreeMode::Direct => Ok(WorktreeGuard {
                repo_root: repo_root.to_path_buf(),
                worktree_path: None,
                worktree_id: None,
            }),
            WorktreeMode::Isolated => {
                let id = format!(
                    "{:x}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos())
                        .unwrap_or(0)
                );

                let wt_dir = repo_root.join(".chilli").join("worktrees").join(&id);
                if let Some(parent) = wt_dir.parent() {
                    fs::create_dir_all(parent).map_err(|e| {
                        format!("Failed to create worktree parent directory: {}", e)
                    })?;
                }

                let branch_name = format!("chilli-wt-{}", id);
                let output = Command::new("git")
                    .args([
                        "worktree",
                        "add",
                        "-b",
                        &branch_name,
                        &wt_dir.to_string_lossy(),
                    ])
                    .current_dir(repo_root)
                    .output();

                match output {
                    Ok(out) if out.status.success() => Ok(WorktreeGuard {
                        repo_root: repo_root.to_path_buf(),
                        worktree_path: Some(wt_dir),
                        worktree_id: Some(id),
                    }),
                    _ => {
                        fs::create_dir_all(&wt_dir).map_err(|e| {
                            format!("Failed to fallback create worktree directory: {}", e)
                        })?;
                        Ok(WorktreeGuard {
                            repo_root: repo_root.to_path_buf(),
                            worktree_path: Some(wt_dir),
                            worktree_id: Some(id),
                        })
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_worktree_guard_sync_files() {
        let repo_dir = tempdir().unwrap();
        let wt_dir = repo_dir
            .path()
            .join(".chilli")
            .join("worktrees")
            .join("wt1");
        fs::create_dir_all(&wt_dir).unwrap();

        let guard = WorktreeGuard {
            repo_root: repo_dir.path().to_path_buf(),
            worktree_path: Some(wt_dir.clone()),
            worktree_id: Some("wt1".to_string()),
        };

        let sub_file = wt_dir.join("src").join("lib.rs");
        fs::create_dir_all(sub_file.parent().unwrap()).unwrap();
        fs::write(&sub_file, "pub fn hello() {}").unwrap();

        let synced = guard.sync_modified_files_to_main().unwrap();
        assert_eq!(synced.len(), 1);
        assert_eq!(synced[0], "src/lib.rs");

        let main_file = repo_dir.path().join("src").join("lib.rs");
        assert!(main_file.exists());
        let content = fs::read_to_string(main_file).unwrap();
        assert_eq!(content, "pub fn hello() {}");
    }
}
