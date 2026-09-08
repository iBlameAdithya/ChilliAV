use chilli_core::engine::AgentEngine;
use chilli_core::state::{AutonomyMode, EngineEvent};
use tempfile::TempDir;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_engine_event_streaming() {
    let temp_dir = TempDir::new().unwrap();
    let mut engine = AgentEngine::new(temp_dir.path()).unwrap();
    engine.set_mock_responses(vec![chilli_core::engine::MockResponse::Text(
        "Mock response stream text".to_string(),
    )]);

    let (tx, mut rx) = mpsc::unbounded_channel::<EngineEvent>();
    engine.set_event_sender(tx);

    assert_eq!(engine.autonomy_mode, AutonomyMode::Auto);

    let res = engine.run_task("Hello test prompt").await.unwrap();
    assert!(!res.final_output.is_empty());

    engine.clear_event_sender();

    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }

    assert!(!events.is_empty());

    let has_turn_started = events
        .iter()
        .any(|e| matches!(e, EngineEvent::TurnStarted { .. }));
    assert!(has_turn_started, "Expected TurnStarted event in stream");
}
