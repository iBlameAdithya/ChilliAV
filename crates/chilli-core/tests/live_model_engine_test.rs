use chilli_core::engine::AgentEngine;
use chilli_model::adapter::{CompletionRequest, ModelAdapter, StopReason, StreamEvent};
use std::sync::Arc;

struct MockLiveAdapter;

#[async_trait::async_trait]
impl ModelAdapter for MockLiveAdapter {
    fn provider_name(&self) -> &'static str {
        "mock"
    }

    fn parse_sse_data(&self, _json_data: &str) -> Result<Vec<StreamEvent>, String> {
        Ok(Vec::new())
    }

    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> Result<tokio_stream::wrappers::ReceiverStream<StreamEvent>, String> {
        let (tx, rx) = tokio::sync::mpsc::channel(10);
        tokio::spawn(async move {
            let _ = tx
                .send(StreamEvent::TextChunk("Hello from model".to_string()))
                .await;
            let _ = tx.send(StreamEvent::Finished(StopReason::EndTurn)).await;
        });
        Ok(tokio_stream::wrappers::ReceiverStream::new(rx))
    }
}

#[tokio::test]
async fn test_agent_engine_runs_with_live_model_adapter() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut engine = AgentEngine::new(temp_dir.path()).unwrap();
    let adapter = Arc::new(MockLiveAdapter);

    let result = engine
        .run_task_with_adapter("say hello", adapter)
        .await
        .unwrap();
    assert!(result.completed);
    assert_eq!(result.final_output, "Hello from model");
}
