use chilli_core::state::{ActiveTaskState, TaskPhase};

#[test]
fn test_working_memory_formatting() {
    let mut state = ActiveTaskState::new("Implement user model");
    state.record_file_modification("src/models/user.rs");
    state.record_file_visit("src/models/mod.rs");
    state.phase = TaskPhase::Execute;

    let summary = state.render_working_memory_summary();
    assert!(summary.contains("[WORKING MEMORY]"));
    assert!(summary.contains("src/models/user.rs"));
    assert!(summary.contains("Execute"));
}
