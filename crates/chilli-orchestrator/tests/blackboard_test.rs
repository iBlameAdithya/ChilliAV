use chilli_orchestrator::blackboard::{Blackboard, BlackboardArtifact};
use serde_json::json;
use std::path::PathBuf;

#[test]
fn test_blackboard_kv_operations() {
    let bb = Blackboard::new();
    bb.set("architecture_style", json!("microservices"));

    assert_eq!(bb.get("architecture_style"), Some(json!("microservices")));
    assert_eq!(bb.get("non_existent"), None);
}

#[test]
fn test_blackboard_artifact_storage() {
    let bb = Blackboard::new();
    bb.add_artifact(BlackboardArtifact {
        task_id: "task_1".to_string(),
        agent_id: "worker_1".to_string(),
        path: PathBuf::from("src/auth.rs"),
        description: Some("Auth module".to_string()),
    });

    let artifacts = bb.get_artifacts_for_task("task_1");
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].path, PathBuf::from("src/auth.rs"));
}

#[test]
fn test_blackboard_messages() {
    let bb = Blackboard::new();
    bb.post_message("Worker 1 finished task_1");
    bb.post_message("Verifier 1 started verifying task_1");

    let messages = bb.get_messages();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0], "Worker 1 finished task_1");

    let cleared_count = bb.clear_messages();
    assert_eq!(cleared_count, 2);
    assert!(bb.get_messages().is_empty());
}

#[test]
fn test_blackboard_count_entries() {
    let bb = Blackboard::new();
    assert_eq!(bb.count_entries(), 0);
    bb.set("k1", json!("v1"));
    bb.set("k2", json!("v2"));
    assert_eq!(bb.count_entries(), 2);
}
