use chilli_context::builder::ContextBuilder;
use chilli_memory::{MemoryCategory, PersistentMemoryEngine};
use tempfile::tempdir;

#[test]
fn test_memory_context_integration_in_builder() {
    let local_dir = tempdir().unwrap();
    let global_dir = tempdir().unwrap();

    let local_db = local_dir.path().join("memory.db");
    let global_db = global_dir.path().join("global_memory.db");

    let engine = PersistentMemoryEngine::with_custom_paths(&local_db, &global_db).unwrap();

    engine
        .store_global(
            MemoryCategory::User,
            "preferred_lang",
            "Rust",
            "settings",
            1.0,
        )
        .unwrap();

    engine
        .store_local(
            MemoryCategory::Project,
            "architecture",
            "microservices",
            "doc",
            0.9,
        )
        .unwrap();

    let mut builder = ContextBuilder::new(4000);
    builder.set_system_prompt("System prompt v1");

    let count = builder.add_memory_context(&engine, None).unwrap();
    assert_eq!(count, 2);

    let prompt = builder.build().unwrap();
    assert!(prompt
        .system_prompt
        .contains("## Persistent Memory Context"));
    assert!(prompt
        .system_prompt
        .contains("[global:user] preferred_lang: Rust"));
    assert!(prompt
        .system_prompt
        .contains("[project:project] architecture: microservices"));
}
