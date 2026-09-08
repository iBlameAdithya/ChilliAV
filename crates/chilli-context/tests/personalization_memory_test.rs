use chilli_context::builder::ContextBuilder;

#[test]
fn test_personalization_memory_budget_cap() {
    let mut builder = ContextBuilder::new(10_000);
    let memories = vec![
        "Prefer Rust over C++".to_string(),
        "Always add unit tests".to_string(),
        "Avoid raw unwrap() calls".to_string(),
        "Use anyhow for error handling".to_string(),
        "Keep functions concise".to_string(),
        "Excess memory item that should be dropped".to_string(),
    ];

    builder.inject_personalization_memories(&memories, 500);
    let context = builder.build().unwrap().system_prompt;

    assert!(context.contains("Prefer Rust over C++"));
    assert!(chilli_context::types::estimate_tokens(&context) <= 10_000);
}
