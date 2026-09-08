use crate::types::PlannerTaskSpec;
use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

pub struct OverlapAnalyzer;

impl OverlapAnalyzer {
    pub fn has_overlap(task_a: &PlannerTaskSpec, task_b: &PlannerTaskSpec) -> bool {
        let set_a: HashSet<_> = task_a.expected_artifacts.iter().collect();
        let set_b: HashSet<_> = task_b.expected_artifacts.iter().collect();
        !set_a.is_disjoint(&set_b)
    }

    pub fn partition_parallel_groups(tasks: &[PlannerTaskSpec]) -> Vec<Vec<PlannerTaskSpec>> {
        let mut groups: Vec<Vec<PlannerTaskSpec>> = Vec::new();

        for task in tasks {
            let mut added = false;
            for group in &mut groups {
                if group
                    .iter()
                    .all(|existing| !Self::has_overlap(existing, task))
                {
                    group.push(task.clone());
                    added = true;
                    break;
                }
            }
            if !added {
                groups.push(vec![task.clone()]);
            }
        }
        groups
    }
}

pub struct WorktreeReconciler;

#[derive(Debug, PartialEq, Eq)]
pub enum MergeResult {
    Success,
    Conflict(String),
    Error(String),
}

impl WorktreeReconciler {
    pub fn merge_branch(repo_root: &Path, branch_name: &str) -> MergeResult {
        let output = Command::new("git")
            .args([
                "merge",
                "--no-ff",
                "-m",
                &format!("Merge {}", branch_name),
                branch_name,
            ])
            .current_dir(repo_root)
            .output();

        match output {
            Ok(out) if out.status.success() => MergeResult::Success,
            Ok(out) => {
                let err_str = String::from_utf8_lossy(&out.stderr).to_string();
                let out_str = String::from_utf8_lossy(&out.stdout).to_string();
                if err_str.contains("CONFLICT") || out_str.contains("CONFLICT") {
                    // Abort conflict merge to leave tree clean for recovery
                    let _ = Command::new("git")
                        .args(["merge", "--abort"])
                        .current_dir(repo_root)
                        .output();
                    MergeResult::Conflict(format!("Merge conflict on {}: {}", branch_name, out_str))
                } else {
                    MergeResult::Error(format!("Git merge failed: {}", err_str))
                }
            }
            Err(e) => MergeResult::Error(format!("Failed to execute git merge: {}", e)),
        }
    }
}
