use chilli_core::loop_detector::{DuplicateLoopDetector, LoopAction};
use serde_json::json;

#[test]
fn test_loop_detector_hard_intervention_thresholds() {
    let mut detector = DuplicateLoopDetector::new();
    let args = json!({"path": "src/main.rs", "old_string": "foo", "new_string": "bar"});

    assert_eq!(
        detector.record_failure("edit_file", &args, "err1"),
        LoopAction::Continue
    );
    assert_eq!(
        detector.record_failure("edit_file", &args, "err2"),
        LoopAction::Continue
    );

    let act3 = detector.record_failure("edit_file", &args, "err3");
    assert!(act3.is_lock());

    let act4 = detector.record_failure("edit_file", &args, "err4");
    assert!(act4.is_terminate());
}
