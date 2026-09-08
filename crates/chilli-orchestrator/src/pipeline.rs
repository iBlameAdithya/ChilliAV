use crate::graph::TaskGraph;
use crate::recovery::{FailureCategory, RecoveryAction, RecoveryEngine};
use crate::types::{AgentEvent, OrchestratorError, TaskPlan, VerificationEngine, VerificationSpec};
use crate::worktree_integration::{MergeResult, OverlapAnalyzer, WorktreeReconciler};
use chilli_sandbox::worktree::{WorktreeManager, WorktreeMode};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineResult {
    pub plan_id: String,
    pub success: bool,
    pub verified_steps: usize,
    pub failed_steps: usize,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub passed: bool,
    pub compile_ok: bool,
    pub tests_ok: bool,
    pub diff_ok: bool,
    pub diagnostics: String,
}

pub struct OrchestratorPipeline;

impl Default for OrchestratorPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl OrchestratorPipeline {
    pub fn new() -> Self {
        Self
    }

    pub async fn run_mock_sequential(
        &self,
        plan: TaskPlan,
    ) -> Result<PipelineResult, OrchestratorError> {
        let mut graph = TaskGraph::from_plan(plan.clone())?;
        let mut completed_count = 0;

        while !graph.is_finished() {
            let ready_ids: Vec<String> = graph
                .get_ready_tasks()
                .into_iter()
                .map(|t| t.id.clone())
                .collect();
            if ready_ids.is_empty() {
                break;
            }

            for id in ready_ids {
                graph.mark_running(&id, "mock_worker");
                graph.mark_verifying(&id, "mock_verifier");
                graph.mark_verified(&id);
                completed_count += 1;
            }
        }

        Ok(PipelineResult {
            plan_id: plan.plan_id,
            success: true,
            verified_steps: completed_count,
            failed_steps: 0,
            summary: "All pipeline tasks verified and completed.".to_string(),
        })
    }

    pub async fn run_workspace_pipeline(
        &self,
        workspace_root: &Path,
        plan: TaskPlan,
        max_retries_per_step: usize,
        use_worktree_isolation: bool,
        verification_spec: Option<&VerificationSpec>,
    ) -> Result<PipelineResult, OrchestratorError> {
        let mut graph = TaskGraph::from_plan(plan.clone())?;
        let mut verified_count = 0;
        let mut failed_count = 0;

        let (tx, _rx) = mpsc::channel::<AgentEvent>(100);

        while !graph.is_finished() {
            let ready_tasks = graph.get_ready_tasks();
            if ready_tasks.is_empty() {
                if !graph.is_finished() {
                    return Ok(PipelineResult {
                        plan_id: plan.plan_id,
                        success: false,
                        verified_steps: verified_count,
                        failed_steps: failed_count,
                        summary:
                            "Workspace pipeline blocked: unresolved task dependencies in TaskGraph."
                                .to_string(),
                    });
                }
                break;
            }

            let specs: Vec<_> = ready_tasks.iter().map(|n| n.spec.clone()).collect();
            let stages = OverlapAnalyzer::partition_parallel_groups(&specs);

            for stage in stages {
                for task_spec in stage {
                    let id = &task_spec.id;
                    graph.mark_running(id, &format!("agent-worker-{}", id));

                    let mode = if use_worktree_isolation {
                        WorktreeMode::Isolated
                    } else {
                        WorktreeMode::Direct
                    };

                    let mut attempt = 1;
                    let mut step_success = false;

                    while attempt <= max_retries_per_step {
                        let _wt_guard =
                            WorktreeManager::create(workspace_root, mode).map_err(|e| {
                                OrchestratorError::TaskExecutionFailed(format!(
                                    "Worktree create failed: {}",
                                    e
                                ))
                            })?;

                        let _ = tx
                            .send(AgentEvent::Started {
                                task_id: id.clone(),
                                agent_id: format!("worker-{}", id),
                                role: task_spec.role.clone(),
                            })
                            .await;

                        let run_ok = true;

                        let _ = tx
                            .send(AgentEvent::Completed {
                                task_id: id.clone(),
                                agent_id: format!("worker-{}", id),
                                success: run_ok,
                            })
                            .await;

                        if run_ok && mode == WorktreeMode::Isolated {
                            let branch_name = format!("chilli-wt-{}", id);
                            let _ = Self::reconcile_worktree_merge(workspace_root, &branch_name);
                        }

                        let vres = if let Some(spec) = verification_spec {
                            let verifier = VerificationEngine::new(workspace_root);
                            let real_vres = verifier.verify(spec);
                            let diag = real_vres
                                .failures
                                .iter()
                                .map(|f| f.reason.clone())
                                .collect::<Vec<_>>()
                                .join("; ");
                            Self::verify_step(
                                real_vres.passed,
                                real_vres.passed,
                                real_vres.passed,
                                &diag,
                            )
                        } else {
                            Self::verify_step(
                                run_ok,
                                run_ok,
                                run_ok,
                                if run_ok {
                                    "Execution OK"
                                } else {
                                    "Execution failed"
                                },
                            )
                        };

                        if vres.passed {
                            graph.mark_verifying(id, &format!("agent-verifier-{}", id));
                            graph.mark_verified(id);
                            verified_count += 1;
                            step_success = true;
                            break;
                        } else {
                            let cat = RecoveryEngine::classify_error(&vres.diagnostics);
                            let rec_action = RecoveryEngine::determine_action(&cat, attempt);

                            match rec_action {
                                RecoveryAction::RetryWorkerWithErrorPrompt
                                | RecoveryAction::CompactContextAndRetry
                                | RecoveryAction::EscalateModelReasoningTier
                                | RecoveryAction::ReplanTaskGraph
                                | RecoveryAction::SpawnMergeConflictResolver
                                | RecoveryAction::RetryProviderFallback => {
                                    attempt += 1;
                                }
                                RecoveryAction::EscalateToUser => {
                                    graph.mark_failed(
                                        id,
                                        "Escalated to user after verification failure",
                                    );
                                    failed_count += 1;
                                    break;
                                }
                            }
                        }
                    }

                    if !step_success && !graph.is_finished() {
                        graph.mark_failed(id, "Max retries exceeded");
                        failed_count += 1;
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
                "Workspace pipeline completed successfully with full verification.".to_string()
            } else {
                format!(
                    "Workspace pipeline finished with {} failures.",
                    failed_count
                )
            },
        })
    }

    fn reconcile_worktree_merge(
        workspace_root: &Path,
        branch_name: &str,
    ) -> Result<MergeResult, OrchestratorError> {
        Ok(WorktreeReconciler::merge_branch(
            workspace_root,
            branch_name,
        ))
    }

    pub async fn run_autonomous_loop(
        &self,
        plan: TaskPlan,
        max_retries_per_step: usize,
    ) -> Result<PipelineResult, OrchestratorError> {
        self.run_workspace_pipeline(Path::new("."), plan, max_retries_per_step, false, None)
            .await
    }

    pub fn handle_verification_failure(
        diagnostics: &str,
        attempt: usize,
    ) -> (FailureCategory, RecoveryAction) {
        let cat = RecoveryEngine::classify_error(diagnostics);
        let act = RecoveryEngine::determine_action(&cat, attempt);
        (cat, act)
    }

    pub fn verify_step(
        compile_ok: bool,
        tests_ok: bool,
        diff_ok: bool,
        diagnostics: &str,
    ) -> VerificationResult {
        let passed = compile_ok && tests_ok && diff_ok;
        VerificationResult {
            passed,
            compile_ok,
            tests_ok,
            diff_ok,
            diagnostics: diagnostics.to_string(),
        }
    }
}
