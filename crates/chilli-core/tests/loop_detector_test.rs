use chilli_core::loop_detector::{DuplicateLoopDetector, LoopAction};

#[test]
fn test_loop_detector_thresholds() {
    let mut detector = DuplicateLoopDetector::default();
    let args = serde_json::json!({"path": "src/main.rs"});

    assert_eq!(
        detector.record_failure("read_file", &args, "Error 1"),
        LoopAction::Continue
    );
    assert_eq!(
        detector.record_failure("read_file", &args, "Error 1"),
        LoopAction::Continue
    );
    assert_eq!(
        detector.record_failure("read_file", &args, "Error 1"),
        LoopAction::LockCall {
            reasoning_escalate: true
        }
    );
    assert_eq!(
        detector.record_failure("read_file", &args, "Error 1"),
        LoopAction::TerminateTask
    );
}

#[test]
fn test_loop_detector_locked_check_and_reset() {
    let mut detector = DuplicateLoopDetector::default();
    let args = serde_json::json!({"path": "src/main.rs"});
    let hash = DuplicateLoopDetector::hash_args(&args);

    assert!(!detector.is_locked(hash));

    detector.record_failure("read_file", &args, "Error 1");
    detector.record_failure("read_file", &args, "Error 1");
    detector.record_failure("read_file", &args, "Error 1");

    assert!(detector.is_locked(hash));

    detector.record_success("read_file", &args);
    // Success removes count tracking for retry
}
