use crate::types::{AgentOutput, OrchestratorError, TaskNode, TaskPlan, TaskStatus};
use std::collections::{HashMap, HashSet};

pub struct TaskGraph {
    pub nodes: HashMap<String, TaskNode>,
}

impl TaskGraph {
    pub fn from_plan(plan: TaskPlan) -> Result<Self, OrchestratorError> {
        let mut nodes = HashMap::new();

        for spec in plan.tasks {
            let node = TaskNode {
                id: spec.id.clone(),
                spec,
                status: TaskStatus::Pending,
                output: None,
            };
            nodes.insert(node.id.clone(), node);
        }

        let graph = Self { nodes };
        graph.validate_dag()?;
        Ok(graph)
    }

    fn validate_dag(&self) -> Result<(), OrchestratorError> {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        for id in self.nodes.keys() {
            if !visited.contains(id) {
                self.check_cycle(id, &mut visited, &mut rec_stack)?;
            }
        }
        Ok(())
    }

    fn check_cycle(
        &self,
        node_id: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
    ) -> Result<(), OrchestratorError> {
        visited.insert(node_id.to_string());
        rec_stack.insert(node_id.to_string());

        if let Some(node) = self.nodes.get(node_id) {
            for dep in &node.spec.depends_on {
                if !self.nodes.contains_key(dep) {
                    return Err(OrchestratorError::InvalidPlan(format!(
                        "Task {} depends on missing task {}",
                        node_id, dep
                    )));
                }
                if !visited.contains(dep) {
                    self.check_cycle(dep, visited, rec_stack)?;
                } else if rec_stack.contains(dep) {
                    return Err(OrchestratorError::CyclicDependency(format!(
                        "Cycle detected between {} and {}",
                        node_id, dep
                    )));
                }
            }
        }

        rec_stack.remove(node_id);
        Ok(())
    }

    pub fn get_ready_tasks(&self) -> Vec<&TaskNode> {
        self.nodes
            .values()
            .filter(|node| {
                if node.status != TaskStatus::Pending {
                    return false;
                }
                node.spec.depends_on.iter().all(|dep_id| {
                    matches!(
                        self.nodes.get(dep_id).map(|n| &n.status),
                        Some(TaskStatus::Completed) | Some(TaskStatus::Verified)
                    )
                })
            })
            .collect()
    }

    pub fn get_ready_tasks_bounded(&self, concurrency_limit: usize) -> Vec<&TaskNode> {
        let ready = self.get_ready_tasks();
        if concurrency_limit == 0 {
            ready
        } else {
            ready.into_iter().take(concurrency_limit).collect()
        }
    }

    pub fn mark_running(&mut self, id: &str, agent_id: &str) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.status = TaskStatus::Running {
                agent_id: agent_id.to_string(),
            };
        }
    }

    pub fn mark_verifying(&mut self, id: &str, agent_id: &str) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.status = TaskStatus::Verifying {
                agent_id: agent_id.to_string(),
            };
        }
    }

    pub fn mark_verified(&mut self, id: &str) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.status = TaskStatus::Verified;
        }
    }

    pub fn mark_completed(&mut self, id: &str, output: Option<AgentOutput>) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.status = TaskStatus::Completed;
            node.output = output;
        }
    }

    pub fn mark_failed(&mut self, id: &str, reason: &str) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.status = TaskStatus::Failed {
                reason: reason.to_string(),
            };
        }
    }

    pub fn mark_cancelled(&mut self, id: &str) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.status = TaskStatus::Cancelled;
        }
    }

    pub fn mark_skipped(&mut self, id: &str) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.status = TaskStatus::Skipped;
        }
    }

    pub fn is_finished(&self) -> bool {
        self.nodes.values().all(|node| {
            matches!(
                node.status,
                TaskStatus::Completed
                    | TaskStatus::Verified
                    | TaskStatus::Failed { .. }
                    | TaskStatus::Skipped
                    | TaskStatus::Cancelled
            )
        })
    }
}
