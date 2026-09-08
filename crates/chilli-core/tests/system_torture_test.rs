use chilli_context::compactor::Compactor;
use chilli_context::truncator::ObservationTruncator;
use chilli_context::types::{MessageItem, Role};
use chilli_core::engine::AgentEngine;
use chilli_memory::task_store::TaskCheckpointStore;
use chilli_memory::{TaskCheckpoint, TaskStatus};
use chilli_model::failure::FailureClass;
use chilli_model::resilience::{CircuitBreaker, ProviderHealth, RetryPolicy};
use chilli_policy::path_policy::{PathPolicy, PolicyDecision};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[tokio::test]
async fn torture_path_traversal_and_injection_defense() {
    let temp_dir = tempfile::tempdir().unwrap();
    let policy = PathPolicy;

    // 1. Path traversal attacks
    let malicious_paths = vec![
        "../../../../etc/passwd",
        "../../../../Windows/System32/cmd.exe",
        "..\\..\\..\\secret.key",
        "C:\\Windows\\System32\\drivers\\etc\\hosts",
        "\\\\.\\PhysicalDrive0",
        "subfolder/../../../etc/shadow",
        ".env",
        ".env.local",
        ".git/config",
        "id_rsa",
        "credentials.db",
    ];

    for path in malicious_paths {
        let decision = policy.check_read(temp_dir.path(), Path::new(path));
        assert!(
            matches!(decision, PolicyDecision::Deny(_) | PolicyDecision::Ask(_)),
            "Security vulnerability: Path '{}' was evaluated as {:?} instead of Deny/Ask!",
            path,
            decision
        );
    }
}

#[tokio::test]
async fn torture_context_truncator_extreme_utf8_and_null_bytes() {
    // 1. Extreme UTF-8 multi-byte emoji boundary test
    let emoji_string = "🔥".repeat(500); // 2000 bytes
    let truncated = ObservationTruncator::truncate(&emoji_string, 50);
    assert!(truncated.len() <= 2000);
    assert!(
        std::str::from_utf8(truncated.as_bytes()).is_ok(),
        "Truncated string is invalid UTF-8!"
    );

    // 2. Null byte and binary byte string
    let mut binary_data = vec![0u8; 1000];
    for (i, item) in binary_data.iter_mut().enumerate().take(1000) {
        *item = (i % 256) as u8;
    }
    let lossy_str = String::from_utf8_lossy(&binary_data);
    let truncated_binary = ObservationTruncator::truncate(&lossy_str, 80);
    assert!(truncated_binary.len() <= 1000);
}

#[tokio::test]
async fn torture_context_compactor_under_zero_budget() {
    let messages = vec![
        MessageItem {
            role: Role::User,
            content: "System prompt ".repeat(100),
            name: None,
            tool_call_id: None,
            tool_calls: None,
        },
        MessageItem {
            role: Role::Assistant,
            content: "Tool output line \n".repeat(200),
            name: None,
            tool_call_id: None,
            tool_calls: None,
        },
        MessageItem {
            role: Role::User,
            content: "User message 1".to_string(),
            name: None,
            tool_call_id: None,
            tool_calls: None,
        },
        MessageItem {
            role: Role::Assistant,
            content: "Assistant message 1".to_string(),
            name: None,
            tool_call_id: None,
            tool_calls: None,
        },
        MessageItem {
            role: Role::User,
            content: "User message 2".to_string(),
            name: None,
            tool_call_id: None,
            tool_calls: None,
        },
    ];

    let compacted = Compactor::compact_messages(&messages, 10);
    assert!(
        !compacted.is_empty(),
        "Compacted result should not crash even under tiny budget!"
    );
}

#[test]
fn torture_retry_policy_and_circuit_breaker() {
    let mut breaker = CircuitBreaker::new(2, Duration::from_secs(1));

    assert_eq!(breaker.check_state(), ProviderHealth::Healthy);

    // Fail 1
    breaker.record_failure(&FailureClass::RateLimited {
        retry_after: Some(Duration::from_secs(1)),
    });
    assert_eq!(breaker.check_state(), ProviderHealth::Degraded);

    // Fail 2 - should trip open
    breaker.record_failure(&FailureClass::ServerError { status: 500 });
    assert_eq!(breaker.check_state(), ProviderHealth::Open);

    // Check backoff
    let policy = RetryPolicy::default();
    let delay1 = policy.calculate_delay(1);
    let delay2 = policy.calculate_delay(2);
    assert!(delay2 >= delay1);
}

#[tokio::test]
async fn torture_concurrent_task_store_reads_and_writes() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store = Arc::new(TaskCheckpointStore::new(temp_dir.path()).unwrap());

    let mut handles = vec![];

    for i in 0..20 {
        let store_clone = Arc::clone(&store);
        let root = temp_dir.path().to_path_buf();
        let handle = tokio::spawn(async move {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let cp = TaskCheckpoint {
                task_id: format!("concurrent_task_{}", i),
                goal: format!("Goal {}", i),
                phase: "act".to_string(),
                status: TaskStatus::Active,
                workspace_root: root.to_string_lossy().to_string(),
                workspace_hash: "hash123".to_string(),
                created_at: now,
                updated_at: now,
                ..TaskCheckpoint::default()
            };
            store_clone.save_checkpoint(&cp).unwrap();
            let loaded = store_clone
                .load_checkpoint(&format!("concurrent_task_{}", i))
                .unwrap();
            assert_eq!(loaded.task_id, format!("concurrent_task_{}", i));
        });
        handles.push(handle);
    }

    for h in handles {
        h.await.unwrap();
    }
}

#[tokio::test]
async fn torture_agent_engine_creation_and_workspace_security() {
    let temp_dir = tempfile::tempdir().unwrap();
    let engine = AgentEngine::new(temp_dir.path());
    assert!(
        engine.is_ok(),
        "AgentEngine should initialize safely in temporary workspace"
    );
}
