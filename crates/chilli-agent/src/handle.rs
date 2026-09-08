use crate::types::{AgentConfig, AgentError, AgentEvent, AgentInput, AgentOutput};
use chilli_core::engine::AgentEngine;
use std::path::Path;
use tokio::sync::mpsc;

pub struct AgentHandle {
    pub config: AgentConfig,
    pub event_tx: mpsc::Sender<AgentEvent>,
}

impl AgentHandle {
    pub fn new(config: AgentConfig, event_tx: mpsc::Sender<AgentEvent>) -> Self {
        Self { config, event_tx }
    }

    pub async fn send_event(&self, event: AgentEvent) {
        let _ = self.event_tx.send(event).await;
    }

    pub async fn run_task_with_engine(
        &self,
        input: AgentInput,
        workspace_root: &Path,
    ) -> Result<AgentOutput, AgentError> {
        self.send_event(AgentEvent::Started {
            task_id: input.task_id.clone(),
            agent_id: self.config.agent_id.clone(),
            role: self.config.role.clone(),
        })
        .await;

        let mut engine = AgentEngine::new(workspace_root)
            .map_err(|e| AgentError::ExecutionFailed(format!("Engine init failed: {}", e)))?;

        if std::env::var("OPENAI_API_KEY").is_err()
            && std::env::var("ANTHROPIC_API_KEY").is_err()
            && std::env::var("GROQ_API_KEY").is_err()
            && std::env::var("GEMINI_API_KEY").is_err()
            && std::env::var("GOOGLE_API_KEY").is_err()
        {
            engine.set_mock_responses(vec![chilli_core::engine::MockResponse::Text(format!(
                "Mock completed task for prompt: {}",
                input.prompt
            ))]);
        }

        let task_res = engine
            .run_task(&input.prompt)
            .await
            .map_err(AgentError::ExecutionFailed)?;

        self.send_event(AgentEvent::TurnCompleted {
            task_id: input.task_id.clone(),
            agent_id: self.config.agent_id.clone(),
            turn: 1,
        })
        .await;

        let output = AgentOutput {
            task_id: input.task_id.clone(),
            agent_id: self.config.agent_id.clone(),
            success: task_res.completed,
            summary: task_res.final_output.clone(),
            artifacts: vec![],
            structured_result: input.structured_data.clone(),
            error: if task_res.completed {
                None
            } else {
                Some("Task execution failed to complete".to_string())
            },
        };

        self.send_event(AgentEvent::Completed {
            task_id: input.task_id.clone(),
            agent_id: self.config.agent_id.clone(),
            success: task_res.completed,
        })
        .await;

        Ok(output)
    }

    pub async fn run_mock(&self, input: AgentInput) -> Result<AgentOutput, AgentError> {
        self.send_event(AgentEvent::Started {
            task_id: input.task_id.clone(),
            agent_id: self.config.agent_id.clone(),
            role: self.config.role.clone(),
        })
        .await;

        self.send_event(AgentEvent::TurnCompleted {
            task_id: input.task_id.clone(),
            agent_id: self.config.agent_id.clone(),
            turn: 1,
        })
        .await;

        let output = AgentOutput {
            task_id: input.task_id.clone(),
            agent_id: self.config.agent_id.clone(),
            success: true,
            summary: format!("Completed task: {}", input.prompt),
            artifacts: vec![],
            structured_result: input.structured_data.clone(),
            error: None,
        };

        self.send_event(AgentEvent::Completed {
            task_id: input.task_id.clone(),
            agent_id: self.config.agent_id.clone(),
            success: true,
        })
        .await;

        Ok(output)
    }
}
