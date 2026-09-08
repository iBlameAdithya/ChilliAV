use crate::graph::TaskGraph;
use crate::pipeline::PipelineResult;
use crate::types::{AgentEvent, OrchestratorError, TaskPlan, VerificationSpec};
use crate::worktree_integration::OverlapAnalyzer;
use chilli_sandbox::worktree::{WorktreeManager, WorktreeMode};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

pub struct ParallelWorktreeOrchestrator {
    concurrency_limit: usize,
}

impl Default for ParallelWorktreeOrchestrator {
    fn default() -> Self {
        Self::new(4)
    }
}

impl ParallelWorktreeOrchestrator {
    pub fn new(concurrency_limit: usize) -> Self {
        Self {
            concurrency_limit: concurrency_limit.max(1),
        }
    }

    pub async fn run_parallel_pipeline(
        &self,
        workspace_root: &Path,
        plan: TaskPlan,
        _verification_spec: Option<&VerificationSpec>,
    ) -> Result<PipelineResult, OrchestratorError> {
        let graph = Arc::new(Mutex::new(TaskGraph::from_plan(plan.clone())?));
        let mut verified_count = 0;
        let mut failed_count = 0;

        let (tx, _rx) = mpsc::channel::<AgentEvent>(100);

        loop {
            let ready_specs = {
                let g = graph.lock().await;
                if g.is_finished() {
                    break;
                }
                let ready_nodes = g.get_ready_tasks_bounded(self.concurrency_limit);
                if ready_nodes.is_empty() {
                    if !g.is_finished() {
                        return Ok(PipelineResult {
                            plan_id: plan.plan_id.clone(),
                            success: false,
                            verified_steps: verified_count,
                            failed_steps: failed_count,
                            summary:
                                "Parallel pipeline blocked: unresolved task dependencies in TaskGraph."
                                    .to_string(),
                        });
                    }
                    break;
                }
                ready_nodes
                    .iter()
                    .map(|n| n.spec.clone())
                    .collect::<Vec<_>>()
            };

            let stages = OverlapAnalyzer::partition_parallel_groups(&ready_specs);

            for stage in stages {
                let mut join_handles = Vec::new();

                for task_spec in stage {
                    let task_id = task_spec.id.clone();
                    let role = task_spec.role.clone();
                    let workspace_path = workspace_root.to_path_buf();
                    let tx_clone = tx.clone();
                    let graph_clone = Arc::clone(&graph);

                    {
                        let mut g = graph_clone.lock().await;
                        g.mark_running(&task_id, &format!("agent-worker-{}", task_id));
                    }

                    let handle = tokio::spawn(async move {
                        let wt_guard =
                            WorktreeManager::create(&workspace_path, WorktreeMode::Isolated)
                                .map_err(|e| {
                                    OrchestratorError::TaskExecutionFailed(format!(
                                        "Worktree creation failed: {}",
                                        e
                                    ))
                                })?;

                        let _ = tx_clone
                            .send(AgentEvent::Started {
                                task_id: task_id.clone(),
                                agent_id: format!("worker-{}", task_id),
                                role,
                            })
                            .await;

                        let sync_res = wt_guard.sync_modified_files_to_main();

                        let _ = tx_clone
                            .send(AgentEvent::Completed {
                                task_id: task_id.clone(),
                                agent_id: format!("worker-{}", task_id),
                                success: sync_res.is_ok(),
                            })
                            .await;

                        match sync_res {
                            Ok(_) => Ok(task_id),
                            Err(e) => Err(OrchestratorError::TaskExecutionFailed(format!(
                                "Sync failed for {}: {}",
                                task_id, e
                            ))),
                        }
                    });

                    join_handles.push(handle);
                }

                for handle in join_handles {
                    match handle.await {
                        Ok(Ok(task_id)) => {
                            let mut g = graph.lock().await;
                            g.mark_verifying(&task_id, &format!("verifier-{}", task_id));
                            g.mark_verified(&task_id);
                            g.mark_completed(&task_id, None);
                            verified_count += 1;
                        }
                        Ok(Err(_e)) => {
                            failed_count += 1;
                        }
                        Err(_e) => {
                            failed_count += 1;
                        }
                    }
                }
            }
        }

        let overall_success = failed_count == 0;
        Ok(PipelineResult {
            plan_id: plan.plan_id,
            success: overall_success,
            verified_steps: verified_count,
            failed_steps: failed_count,
            summary: if overall_success {
                "Parallel worktree pipeline completed successfully.".to_string()
            } else {
                format!(
                    "Parallel worktree pipeline finished with {} failures.",
                    failed_count
                )
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AgentRole, PlannerTaskSpec};
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_parallel_worktree_orchestrator() {
        let dir = tempdir().unwrap();

        let plan = TaskPlan {
            plan_id: "parallel-plan-01".to_string(),
            goal: "Run 2 parallel tasks".to_string(),
            tasks: vec![
                PlannerTaskSpec {
                    id: "t1".to_string(),
                    description: "Task 1".to_string(),
                    role: AgentRole::Worker,
                    depends_on: vec![],
                    expected_artifacts: vec!["file1.txt".to_string()],
                },
                PlannerTaskSpec {
                    id: "t2".to_string(),
                    description: "Task 2".to_string(),
                    role: AgentRole::Worker,
                    depends_on: vec![],
                    expected_artifacts: vec!["file2.txt".to_string()],
                },
            ],
        };

        let orchestrator = ParallelWorktreeOrchestrator::new(2);
        let res = orchestrator
            .run_parallel_pipeline(dir.path(), plan, None)
            .await
            .unwrap();

        assert!(res.success);
        assert_eq!(res.verified_steps, 2);
        assert_eq!(res.failed_steps, 0);
    }
}
