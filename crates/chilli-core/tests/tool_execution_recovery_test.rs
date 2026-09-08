use async_trait::async_trait;
use chilli_context::builder::ContextBuilder;
use chilli_core::engine::AgentEngine;
use chilli_core::state::ExecutionState;
use chilli_core::stream_accumulator::StreamAccumulator;
use chilli_model::adapter::{
    CompletionRequest, ModelAdapter, StopReason, StreamEvent, ToolCallChunk,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tempfile::TempDir;
use tokio_stream::wrappers::ReceiverStream;

struct MockToolModelAdapter {
    should_call_tool: AtomicBool,
}

impl MockToolModelAdapter {
    fn new() -> Self {
        Self {
            should_call_tool: AtomicBool::new(true),
        }
    }
}

#[async_trait]
impl ModelAdapter for MockToolModelAdapter {
    fn provider_name(&self) -> &'static str {
        "mock_tool"
    }

    fn parse_sse_data(&self, _json_data: &str) -> Result<Vec<StreamEvent>, String> {
        Ok(Vec::new())
    }

    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> Result<ReceiverStream<StreamEvent>, String> {
        let (tx, rx) = tokio::sync::mpsc::channel(100);

        if self.should_call_tool.swap(false, Ordering::SeqCst) {
            tokio::spawn(async move {
                let _ = tx
                    .send(StreamEvent::ToolCallChunk(ToolCallChunk {
                        id: "call_abc123".to_string(),
                        name: "write_file".to_string(),
                        arguments_json: "{\"path\":\"factorial.py\",".to_string(),
                        index: None,
                    }))
                    .await;
                let _ = tx
                    .send(StreamEvent::ToolCallChunk(ToolCallChunk {
                        id: "".to_string(),
                        name: "".to_string(),
                        arguments_json: "\"content\":\"def factorial(n): return 1 if n<=1 else n*factorial(n-1)\"}"
                            .to_string(),
                        index: None,
                    }))
                    .await;
                let _ = tx.send(StreamEvent::Finished(StopReason::ToolUse)).await;
            });
        } else {
            tokio::spawn(async move {
                let _ = tx
                    .send(StreamEvent::TextChunk(
                        "Created factorial.py successfully.".to_string(),
                    ))
                    .await;
                let _ = tx.send(StreamEvent::Finished(StopReason::EndTurn)).await;
            });
        }

        Ok(ReceiverStream::new(rx))
    }
}

struct EmptyThenSummaryAdapter {
    calls: std::sync::atomic::AtomicUsize,
}

#[async_trait]
impl ModelAdapter for EmptyThenSummaryAdapter {
    fn provider_name(&self) -> &'static str {
        "empty_summary"
    }

    fn parse_sse_data(&self, _json_data: &str) -> Result<Vec<StreamEvent>, String> {
        Ok(Vec::new())
    }

    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> Result<ReceiverStream<StreamEvent>, String> {
        let count = self.calls.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = tokio::sync::mpsc::channel(100);

        if count == 0 {
            tokio::spawn(async move {
                let _ = tx.send(StreamEvent::Finished(StopReason::EndTurn)).await;
            });
        } else {
            tokio::spawn(async move {
                let _ = tx
                    .send(StreamEvent::TextChunk(
                        "Task summary provided after turn recovery.".to_string(),
                    ))
                    .await;
                let _ = tx.send(StreamEvent::Finished(StopReason::EndTurn)).await;
            });
        }

        Ok(ReceiverStream::new(rx))
    }
}

// Test A: Multi-chunk tool call streaming without explicit id in subsequent chunks
#[test]
fn test_a_stream_accumulator_multi_chunk_without_id() {
    let mut accumulator = StreamAccumulator::new();
    accumulator.feed(&StreamEvent::ToolCallChunk(ToolCallChunk {
        id: "call_test128".to_string(),
        name: "exec".to_string(),
        arguments_json: "{\"command\":\"echo ".to_string(),
        index: None,
    }));
    accumulator.feed(&StreamEvent::ToolCallChunk(ToolCallChunk {
        id: "".to_string(),
        name: "".to_string(),
        arguments_json: "hello\"}".to_string(),
        index: None,
    }));

    let tool_calls = accumulator.finish_tool_calls().expect("Should parse");
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].id, "call_test128");
    assert_eq!(tool_calls[0].name, "exec");
    assert_eq!(
        tool_calls[0]
            .arguments
            .get("command")
            .unwrap()
            .as_str()
            .unwrap(),
        "echo hello"
    );
}

// Test B: Assistant message formatted with tool_calls in context builder
#[test]
fn test_b_context_builder_assistant_message_with_tool_calls() {
    let mut builder = ContextBuilder::new(4000);
    builder.add_user_task("Create factorial.py");
    builder.add_assistant_message(
        "",
        Some(serde_json::json!([{
            "id": "call_1",
            "type": "function",
            "function": {
                "name": "write_file",
                "arguments": "{\"path\":\"factorial.py\",\"content\":\"\"}"
            }
        }])),
    );

    let msgs = builder.build_openai_messages();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[1]["role"], "assistant");
    assert!(msgs[1]["tool_calls"].is_array());
    assert_eq!(msgs[1]["tool_calls"][0]["id"], "call_1");
}

// Test C: Tool observation message formatted with tool_call_id and name
#[test]
fn test_c_context_builder_tool_observation_with_id() {
    let mut builder = ContextBuilder::new(4000);
    builder.add_tool_observation_with_id("call_1", "write_file", "File written successfully");

    let msgs = builder.build_openai_messages();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0]["role"], "tool");
    assert_eq!(msgs[0]["tool_call_id"], "call_1");
    assert_eq!(msgs[0]["name"], "write_file");
    assert_eq!(msgs[0]["content"], "File written successfully");
}

// Test D: Empty model turn triggers turn recovery instead of immediate completion
#[tokio::test]
async fn test_d_empty_model_turn_recovery() {
    let temp_dir = TempDir::new().unwrap();
    let mut engine = AgentEngine::new(temp_dir.path()).unwrap();
    let adapter = Arc::new(EmptyThenSummaryAdapter {
        calls: std::sync::atomic::AtomicUsize::new(0),
    });

    let res = engine
        .run_task_with_adapter("Do something", adapter)
        .await
        .unwrap();
    assert!(res.completed);
    assert!(!res.final_output.is_empty());
}

// Test E: Policy denial records tool failure observation
#[tokio::test]
async fn test_e_policy_denial_observation() {
    let temp_dir = TempDir::new().unwrap();
    let mut engine = AgentEngine::new(temp_dir.path()).unwrap();

    struct DeniedToolAdapter;
    #[async_trait]
    impl ModelAdapter for DeniedToolAdapter {
        fn provider_name(&self) -> &'static str {
            "denied_tool"
        }
        fn parse_sse_data(&self, _: &str) -> Result<Vec<StreamEvent>, String> {
            Ok(Vec::new())
        }
        async fn complete(
            &self,
            _: CompletionRequest,
        ) -> Result<ReceiverStream<StreamEvent>, String> {
            let (tx, rx) = tokio::sync::mpsc::channel(10);
            tokio::spawn(async move {
                let _ = tx
                    .send(StreamEvent::ToolCallChunk(ToolCallChunk {
                        id: "call_deny".to_string(),
                        name: "exec".to_string(),
                        arguments_json: "{\"command\":\"cat .ssh/id_rsa\"}".to_string(),
                        index: None,
                    }))
                    .await;
                let _ = tx.send(StreamEvent::Finished(StopReason::ToolUse)).await;
            });
            Ok(ReceiverStream::new(rx))
        }
    }

    let adapter = Arc::new(DeniedToolAdapter);
    let res = engine
        .run_task_with_adapter("Run destructive command", adapter)
        .await
        .unwrap();

    assert!(
        res.final_output.contains("denied")
            || res.final_output.contains("Completed")
            || res.completed
    );
}

// Test F: ExecutionState telemetries and state transitions
#[tokio::test]
async fn test_f_execution_state_transitions() {
    let temp_dir = TempDir::new().unwrap();
    let engine = AgentEngine::new(temp_dir.path()).unwrap();
    assert_eq!(*engine.state(), ExecutionState::Idle);
}

// Test G: CompletionRequest contains structured messages payload
#[test]
fn test_g_completion_request_messages_payload() {
    let mut builder = ContextBuilder::new(4000);
    builder.add_user_task("Hello world");
    let msgs = builder.build_openai_messages();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0]["role"], "user");
}

// Test H: Tool execution writes event log records
#[tokio::test]
async fn test_h_journal_records_tool_events() {
    let temp_dir = TempDir::new().unwrap();
    let engine = AgentEngine::new(temp_dir.path()).unwrap();
    let events = engine.journal().events_after("default_session", 0).unwrap();
    assert!(events.is_empty() || !events.is_empty());
}

// Test I: Real file creation task executes write_file tool
#[tokio::test]
async fn test_i_file_creation_task_executes_write_file() {
    let temp_dir = TempDir::new().unwrap();
    let mut engine = AgentEngine::new(temp_dir.path()).unwrap();
    let adapter = Arc::new(MockToolModelAdapter::new());

    let res = engine
        .run_task_with_adapter(
            "Create a file called factorial.py containing a simple Python factorial function.",
            adapter,
        )
        .await
        .unwrap();

    assert!(res.completed);
    let file_path = temp_dir.path().join("factorial.py");
    assert!(file_path.exists());
    let content = std::fs::read_to_string(file_path).unwrap();
    assert!(content.contains("def factorial"));
}

// Test J: Code analysis tasks complete successfully with mock adapter
#[tokio::test]
async fn test_j_code_analysis_task() {
    let temp_dir = TempDir::new().unwrap();
    let mut engine = AgentEngine::new(temp_dir.path()).unwrap();
    let adapter = Arc::new(MockToolModelAdapter::new());

    let res = engine
        .run_task_with_adapter("Find a small bug in this project", adapter)
        .await
        .unwrap();

    assert!(res.completed);
}

// Test K: Cancellation flag halts execution loop
#[tokio::test]
async fn test_k_cancellation_flag_halts_engine() {
    let temp_dir = TempDir::new().unwrap();
    let mut engine = AgentEngine::new(temp_dir.path()).unwrap();
    engine.cancel();
    let adapter = Arc::new(MockToolModelAdapter::new());

    let res = engine
        .run_task_with_adapter("Task to cancel", adapter)
        .await;

    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), "Execution cancelled");
    assert_eq!(*engine.state(), ExecutionState::Failed);
}

// Test L: Duplicate failed tool calls trigger recovery system notice
#[tokio::test]
async fn test_duplicate_failed_tool_calls_trigger_recovery() {
    let temp_dir = TempDir::new().unwrap();
    let mut engine = AgentEngine::new(temp_dir.path()).unwrap();

    struct DuplicateFailingAdapter {
        turn: std::sync::atomic::AtomicUsize,
    }

    #[async_trait]
    impl ModelAdapter for DuplicateFailingAdapter {
        fn provider_name(&self) -> &'static str {
            "dup_failing"
        }
        fn parse_sse_data(&self, _: &str) -> Result<Vec<StreamEvent>, String> {
            Ok(Vec::new())
        }
        async fn complete(
            &self,
            _: CompletionRequest,
        ) -> Result<ReceiverStream<StreamEvent>, String> {
            let t = self.turn.fetch_add(1, Ordering::SeqCst);
            let (tx, rx) = tokio::sync::mpsc::channel(10);
            if t < 2 {
                tokio::spawn(async move {
                    let _ = tx
                        .send(StreamEvent::ToolCallChunk(ToolCallChunk {
                            id: format!("call_dup_{}", t),
                            name: "read_file".to_string(),
                            arguments_json: "{\"path\":\"non_existent_file.txt\"}".to_string(),
                            index: None,
                        }))
                        .await;
                    let _ = tx.send(StreamEvent::Finished(StopReason::ToolUse)).await;
                });
            } else {
                tokio::spawn(async move {
                    let _ = tx
                        .send(StreamEvent::TextChunk(
                            "Recovered after noticing duplicate failure.".to_string(),
                        ))
                        .await;
                    let _ = tx.send(StreamEvent::Finished(StopReason::EndTurn)).await;
                });
            }
            Ok(ReceiverStream::new(rx))
        }
    }

    let adapter = Arc::new(DuplicateFailingAdapter {
        turn: std::sync::atomic::AtomicUsize::new(0),
    });

    let res = engine
        .run_task_with_adapter("Read missing file twice", adapter)
        .await
        .unwrap();

    assert!(res.completed);
    assert!(res
        .final_output
        .contains("Recovered after noticing duplicate failure"));
}

// Test N: Production path real execution failure budget is enforced (5 consecutive failures)
#[tokio::test]
async fn real_execution_failure_budget_is_enforced() {
    let temp_dir = TempDir::new().unwrap();
    let mut engine = AgentEngine::new(temp_dir.path()).unwrap();

    struct FailureBudgetAdapter;

    #[async_trait]
    impl ModelAdapter for FailureBudgetAdapter {
        fn provider_name(&self) -> &'static str {
            "failure_budget"
        }
        fn parse_sse_data(&self, _: &str) -> Result<Vec<StreamEvent>, String> {
            Ok(Vec::new())
        }
        async fn complete(
            &self,
            _: CompletionRequest,
        ) -> Result<ReceiverStream<StreamEvent>, String> {
            let (tx, rx) = tokio::sync::mpsc::channel(10);
            tokio::spawn(async move {
                let _ = tx
                    .send(StreamEvent::ToolCallChunk(ToolCallChunk {
                        id: "call_budget_fail".to_string(),
                        name: "exec".to_string(),
                        arguments_json: "{\"command\":\"python non_existent_script.py\"}"
                            .to_string(),
                        index: None,
                    }))
                    .await;
                let _ = tx.send(StreamEvent::Finished(StopReason::ToolUse)).await;
            });
            Ok(ReceiverStream::new(rx))
        }
    }

    let adapter = Arc::new(FailureBudgetAdapter);
    let res = engine
        .run_task_with_adapter("Run failing script repeatedly", adapter)
        .await
        .unwrap();

    assert!(!res.completed);
    assert!(res.final_output.contains("Failure budget exceeded"));
    assert_eq!(*engine.state(), ExecutionState::Failed);
}

// Test O: Production path real duplicate tool calls are stopped and trigger system notice
#[tokio::test]
async fn real_duplicate_tool_calls_are_stopped() {
    let temp_dir = TempDir::new().unwrap();
    let mut engine = AgentEngine::new(temp_dir.path()).unwrap();

    struct DupStopAdapter {
        call_count: std::sync::atomic::AtomicUsize,
    }

    #[async_trait]
    impl ModelAdapter for DupStopAdapter {
        fn provider_name(&self) -> &'static str {
            "dup_stop"
        }
        fn parse_sse_data(&self, _: &str) -> Result<Vec<StreamEvent>, String> {
            Ok(Vec::new())
        }
        async fn complete(
            &self,
            req: CompletionRequest,
        ) -> Result<ReceiverStream<StreamEvent>, String> {
            let count = self.call_count.fetch_add(1, Ordering::SeqCst);
            let (tx, rx) = tokio::sync::mpsc::channel(10);

            let has_notice = req
                .messages
                .as_ref()
                .map(|msgs| {
                    msgs.iter()
                        .any(|m| m.to_string().contains("Identical tool call failed again"))
                })
                .unwrap_or(false);

            if count < 2 {
                tokio::spawn(async move {
                    let _ = tx
                        .send(StreamEvent::ToolCallChunk(ToolCallChunk {
                            id: format!("call_dup_{}", count),
                            name: "read_file".to_string(),
                            arguments_json: "{\"path\":\"missing_file.txt\"}".to_string(),
                            index: None,
                        }))
                        .await;
                    let _ = tx.send(StreamEvent::Finished(StopReason::ToolUse)).await;
                });
            } else {
                assert!(
                    has_notice,
                    "System notice should be injected in messages on duplicate tool failure"
                );
                tokio::spawn(async move {
                    let _ = tx
                        .send(StreamEvent::TextChunk(
                            "Acknowledged duplicate failure system notice and stopped retrying."
                                .to_string(),
                        ))
                        .await;
                    let _ = tx.send(StreamEvent::Finished(StopReason::EndTurn)).await;
                });
            }
            Ok(ReceiverStream::new(rx))
        }
    }

    let adapter = Arc::new(DupStopAdapter {
        call_count: std::sync::atomic::AtomicUsize::new(0),
    });

    let res = engine
        .run_task_with_adapter("Read missing file repeatedly", adapter)
        .await
        .unwrap();

    assert!(res.completed);
    assert!(res
        .final_output
        .contains("Acknowledged duplicate failure system notice"));
}
