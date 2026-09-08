use chilli_core::engine::AgentEngine;
use chilli_core::state::{ControlEvent, EngineEvent, PlanApprovalResponse, TaskPhase};
use tempfile::tempdir;

#[tokio::test]
async fn test_bidirectional_engine_control_channel() {
    let temp = tempdir().unwrap();
    let mut engine = AgentEngine::new(temp.path()).unwrap();

    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    let (control_tx, control_rx) = tokio::sync::mpsc::unbounded_channel();

    engine.set_event_sender(event_tx);
    engine.set_control_receiver(control_rx);

    engine.update_phase(TaskPhase::Plan);
    let event = event_rx.try_recv().unwrap();
    assert!(matches!(
        event,
        EngineEvent::PhaseChanged {
            phase: TaskPhase::Plan
        }
    ));

    control_tx
        .send(ControlEvent::PlanApproval(PlanApprovalResponse::Approve))
        .unwrap();
}
