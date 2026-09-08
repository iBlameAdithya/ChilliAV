use chilli_orchestrator::types::{AgentRole, PlannerTaskSpec};
use chilli_orchestrator::worktree_integration::OverlapAnalyzer;

#[test]
fn test_detect_file_overlap() {
    let t1 = PlannerTaskSpec {
        id: "t1".to_string(),
        description: "Fix auth".to_string(),
        role: AgentRole::Worker,
        depends_on: vec![],
        expected_artifacts: vec!["src/auth.rs".to_string(), "src/main.rs".to_string()],
    };

    let t2 = PlannerTaskSpec {
        id: "t2".to_string(),
        description: "Refactor auth header".to_string(),
        role: AgentRole::Worker,
        depends_on: vec![],
        expected_artifacts: vec!["src/auth.rs".to_string()],
    };

    let t3 = PlannerTaskSpec {
        id: "t3".to_string(),
        description: "Update database pool".to_string(),
        role: AgentRole::Worker,
        depends_on: vec![],
        expected_artifacts: vec!["src/db.rs".to_string()],
    };

    assert!(OverlapAnalyzer::has_overlap(&t1, &t2));
    assert!(!OverlapAnalyzer::has_overlap(&t1, &t3));
}

#[test]
fn test_partition_parallel_groups() {
    let t1 = PlannerTaskSpec {
        id: "t1".to_string(),
        description: "Task 1".to_string(),
        role: AgentRole::Worker,
        depends_on: vec![],
        expected_artifacts: vec!["src/a.rs".to_string()],
    };

    let t2 = PlannerTaskSpec {
        id: "t2".to_string(),
        description: "Task 2".to_string(),
        role: AgentRole::Worker,
        depends_on: vec![],
        expected_artifacts: vec!["src/a.rs".to_string()],
    };

    let t3 = PlannerTaskSpec {
        id: "t3".to_string(),
        description: "Task 3".to_string(),
        role: AgentRole::Worker,
        depends_on: vec![],
        expected_artifacts: vec!["src/b.rs".to_string()],
    };

    let groups = OverlapAnalyzer::partition_parallel_groups(&[t1, t2, t3]);
    assert_eq!(groups.len(), 2);
    // Group 1 has t1 & t3 (no overlap)
    // Group 2 has t2 (overlaps with t1)
    assert_eq!(groups[0].len(), 2);
    assert_eq!(groups[1].len(), 1);
}

#[test]
fn test_worktree_reconciler_non_existent_branch() {
    use chilli_orchestrator::worktree_integration::{MergeResult, WorktreeReconciler};
    use std::path::Path;

    let res = WorktreeReconciler::merge_branch(Path::new("."), "non_existent_branch_12345");
    assert!(matches!(res, MergeResult::Error(_)));
}
