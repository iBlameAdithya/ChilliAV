use chilli_memory::{MemoryCategory, MemoryQualityFilter, PersistentMemoryEngine};
use tempfile::tempdir;

#[test]
fn test_dual_tier_persistent_memory_engine() {
    let local_dir = tempdir().unwrap();
    let global_dir = tempdir().unwrap();

    let local_db = local_dir.path().join("memory.db");
    let global_db = global_dir.path().join("global_memory.db");

    let engine = PersistentMemoryEngine::with_custom_paths(&local_db, &global_db).unwrap();

    // Store global preference
    let global_entry = engine
        .store_global(
            MemoryCategory::User,
            "theme_setting",
            "dark_mode_enabled",
            "user_settings",
            1.0,
        )
        .unwrap();

    assert!(global_entry.is_global);
    assert_eq!(global_entry.category, MemoryCategory::User);
    assert_eq!(global_entry.key, "theme_setting");
    assert_eq!(global_entry.content, "dark_mode_enabled");

    // Store local project preference with same key
    let local_entry = engine
        .store_local(
            MemoryCategory::User,
            "theme_setting",
            "light_mode_for_docs",
            "project_override",
            0.9,
        )
        .unwrap();

    assert!(!local_entry.is_global);
    assert_eq!(local_entry.content, "light_mode_for_docs");

    // Store local project fact
    engine
        .store_local(
            MemoryCategory::Project,
            "build_tool",
            "cargo build --workspace",
            "repo_scan",
            1.0,
        )
        .unwrap();

    // Recall global
    let global_recalled = engine
        .recall_global(Some(MemoryCategory::User), None)
        .unwrap();
    assert_eq!(global_recalled.len(), 1);
    assert_eq!(global_recalled[0].content, "dark_mode_enabled");

    // Recall local
    let local_recalled = engine
        .recall_local(Some(MemoryCategory::User), None)
        .unwrap();
    assert_eq!(local_recalled.len(), 1);
    assert_eq!(local_recalled[0].content, "light_mode_for_docs");

    // Recall combined - local theme should override global theme
    let combined = engine.recall_combined(None, None).unwrap();
    assert_eq!(combined.len(), 2);

    let theme_mem = combined.iter().find(|m| m.key == "theme_setting").unwrap();
    assert_eq!(theme_mem.content, "light_mode_for_docs");
    assert!(!theme_mem.is_global);

    // Query test
    let queried = engine.recall_combined(None, Some("cargo")).unwrap();
    assert_eq!(queried.len(), 1);
    assert_eq!(queried[0].key, "build_tool");

    // Delete local
    let deleted = engine.delete_local(&local_entry.id).unwrap();
    assert!(deleted);

    // Recall combined after local theme deleted - global theme should emerge
    let combined_after_delete = engine.recall_combined(None, None).unwrap();
    let theme_after = combined_after_delete
        .iter()
        .find(|m| m.key == "theme_setting")
        .unwrap();
    assert_eq!(theme_after.content, "dark_mode_enabled");
    assert!(theme_after.is_global);
}

#[test]
fn test_memory_quality_filter() {
    // Rejects secrets & credentials
    assert!(!MemoryQualityFilter::is_high_quality(
        MemoryCategory::Project,
        "db_secret",
        "AWS_SECRET_ACCESS_KEY=12345"
    ));
    assert!(!MemoryQualityFilter::is_high_quality(
        MemoryCategory::Project,
        "dotenv",
        "reading .env file credentials"
    ));

    // Rejects transient build logs
    assert!(!MemoryQualityFilter::is_high_quality(
        MemoryCategory::Session,
        "log",
        "Finished dev [unoptimized + debuginfo] target(s) in 0.5s"
    ));

    // Rejects empty/too short noise
    assert!(!MemoryQualityFilter::is_high_quality(
        MemoryCategory::Session,
        "ok",
        "ok"
    ));

    // Accepts high quality architectural decision
    assert!(MemoryQualityFilter::is_high_quality(
        MemoryCategory::Architecture,
        "storage_engine",
        "Use SQLite WAL mode for persistent memory storage"
    ));
}

#[test]
fn test_memory_staleness_and_conflict_handling() {
    let local_dir = tempdir().unwrap();
    let global_dir = tempdir().unwrap();

    let local_db = local_dir.path().join("memory.db");
    let global_db = global_dir.path().join("global_memory.db");

    let engine = PersistentMemoryEngine::with_custom_paths(&local_db, &global_db).unwrap();

    let entry1 = engine
        .store_local(
            MemoryCategory::Architecture,
            "context_limit",
            "Context limit set to 8000 tokens",
            "old_spec",
            1.0,
        )
        .unwrap();

    assert!(!entry1.is_stale);

    // Mark entry1 as stale when superseded by entry2
    let entry2 = engine
        .store_local(
            MemoryCategory::Architecture,
            "context_limit_v2",
            "Context limit expanded to 16000 tokens",
            "new_spec",
            1.0,
        )
        .unwrap();

    let marked = engine
        .mark_stale_local(&entry1.id, Some(&entry2.id))
        .unwrap();
    assert!(marked);

    // recall_combined should filter out stale memories by default
    let active_memories = engine.recall_combined(None, None).unwrap();
    assert_eq!(active_memories.len(), 1);
    assert_eq!(active_memories[0].key, "context_limit_v2");
}

#[test]
fn test_relevance_ranked_recall() {
    let local_dir = tempdir().unwrap();
    let global_dir = tempdir().unwrap();

    let local_db = local_dir.path().join("memory.db");
    let global_db = global_dir.path().join("global_memory.db");

    let engine = PersistentMemoryEngine::with_custom_paths(&local_db, &global_db).unwrap();

    engine
        .store_local(
            MemoryCategory::Architecture,
            "codegraph_ranking",
            "Use CodeGraph symbols for tier3 context assembly",
            "arch_doc",
            1.0,
        )
        .unwrap();

    engine
        .store_local(
            MemoryCategory::Reference,
            "cli_colors",
            "Use standard ANSI colors for CLI output",
            "ui_spec",
            1.0,
        )
        .unwrap();

    let ranked = engine
        .recall_ranked(
            "Fix CodeGraph symbol ranking",
            &["builder.rs".to_string()],
            &["CodeGraph".to_string()],
            5,
        )
        .unwrap();

    assert!(!ranked.is_empty());
    assert_eq!(ranked[0].key, "codegraph_ranking");
}
