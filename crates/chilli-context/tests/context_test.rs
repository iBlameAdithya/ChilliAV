use chilli_context::builder::ContextBuilder;
use chilli_context::types::Role;

#[test]
fn test_context_assembly_truncation_and_cache_alignment() {
    let mut builder = ContextBuilder::new(100_000);

    builder.set_system_prompt("You are Chilli AI agent.");
    builder.set_tools_declaration(serde_json::json!([{"name": "read_file"}]));
    builder.add_user_task("Fix bug in main.rs");

    // Huge observation log that exceeds limit
    let huge_log = "log line\n".repeat(5000);
    builder.add_observation("read_file", &huge_log);

    let assembled = builder.build().unwrap();

    // Verification 1: System prompt and Tools fixed for prompt cache alignment
    assert_eq!(assembled.system_prompt, "You are Chilli AI agent.");
    assert!(assembled.tools_json.to_string().contains("read_file"));

    // Verification 2: Observation log truncated cleanly
    assert!(assembled
        .messages
        .iter()
        .any(|m| m.content.contains("[Truncated")));
    assert!(assembled.estimated_tokens < 100_000);
}

#[test]
fn test_compactor_summarizes_middle_turns_when_token_threshold_exceeded() {
    let mut builder = ContextBuilder::new(2_000);

    builder.set_system_prompt("You are Chilli AI agent.");
    builder.set_tools_declaration(serde_json::json!([{"name": "test_tool"}]));
    builder.add_user_task("Original task instruction");

    for _i in 0..20 {
        builder.add_message(
            Role::Assistant,
            &"Turn response thinking and acting ".repeat(20),
        );
        builder.add_message(Role::User, &"Turn user feedback or tool output ".repeat(20));
    }

    let assembled = builder.build().unwrap();
    assert!(assembled
        .messages
        .iter()
        .any(|m| m.content.contains("[Structured Milestone Summary]")));
    assert_eq!(assembled.system_prompt, "You are Chilli AI agent.");
}

#[test]
fn test_independent_repl_turns_do_not_share_task_history() {
    let mut builder = ContextBuilder::new(100_000);
    builder.set_system_prompt("System prompt");
    builder.add_user_task("Create factorial.py and run with 5");
    builder.add_assistant_message("I will create factorial.py", None);

    // Clear turn messages between REPL turns
    builder.clear_messages();
    builder.add_user_task("What files are inside the docs directory?");

    let msgs = builder.build_openai_messages();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0]["role"], "user");
    assert_eq!(
        msgs[0]["content"],
        "What files are inside the docs directory?"
    );
}

#[test]
fn test_previous_tool_observations_do_not_leak_to_next_task() {
    let mut builder = ContextBuilder::new(100_000);
    builder.set_system_prompt("System prompt");
    builder.add_user_task("Task 1");
    builder.add_tool_observation_with_id("call_1", "write_file", "File factorial.py written");

    builder.clear_messages();
    builder.add_user_task("Task 2");

    let msgs = builder.build_openai_messages();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0]["content"], "Task 2");
    assert!(!msgs.iter().any(|m| m.to_string().contains("factorial.py")));
}

#[test]
fn test_stable_system_context_survives_turn_reset() {
    let mut builder = ContextBuilder::new(100_000);
    builder.set_system_prompt("You are Chilli AI agent.");
    builder.set_tools_declaration(serde_json::json!([{"name": "exec"}]));

    builder.add_user_task("Turn 1 task");
    builder.clear_messages();
    builder.add_user_task("Turn 2 task");

    let prompt = builder.build().unwrap();
    assert_eq!(prompt.system_prompt, "You are Chilli AI agent.");
    assert!(prompt.tools_json.to_string().contains("exec"));
    assert_eq!(prompt.messages.len(), 1);
    assert_eq!(prompt.messages[0].content, "Turn 2 task");
}

#[test]
fn test_current_task_tool_observations_are_preserved() {
    let mut builder = ContextBuilder::new(100_000);
    builder.set_system_prompt("System prompt");
    builder.add_user_task("Current task");
    builder.add_tool_observation_with_id("call_1", "exec", "Command output ok");

    let msgs = builder.build_openai_messages();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0]["role"], "user");
    assert_eq!(msgs[1]["role"], "tool");
    assert_eq!(msgs[1]["content"], "Command output ok");
}

#[test]
fn test_deterministic_tool_schema_sorting() {
    let mut builder = ContextBuilder::new(100_000);
    let unsorted_tools = serde_json::json!([
        {"name": "write_file"},
        {"name": "exec"},
        {"name": "read_file"},
        {"name": "glob"}
    ]);
    builder.set_tools_declaration(unsorted_tools);

    let prompt = builder.build().unwrap();
    let tools_arr = prompt.tools_json.as_array().unwrap();
    let names: Vec<&str> = tools_arr
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();

    assert_eq!(names, vec!["exec", "glob", "read_file", "write_file"]);
}

#[test]
fn test_static_and_dynamic_system_prompt_separation() {
    let mut builder = ContextBuilder::new(100_000);
    builder.set_static_system_prompt("Tier 1 Invariant System Prompt");
    builder.set_dynamic_system_prompt("Tier 3 Dynamic Context Notice");
    builder.set_tools_declaration(serde_json::json!([{"name": "exec"}]));
    builder.add_user_task("Run test");

    let cache_aligned = builder.build_cache_aligned().unwrap();
    assert_eq!(cache_aligned.tier1_system, "Tier 1 Invariant System Prompt");
    assert!(cache_aligned
        .tier3_ast_graph
        .contains("Tier 3 Dynamic Context Notice"));
    assert!(cache_aligned.static_prefix_tokens > 0);

    let formatted = builder.build().unwrap();
    assert!(formatted
        .system_prompt
        .starts_with("Tier 1 Invariant System Prompt"));
    assert!(formatted
        .system_prompt
        .contains("Tier 3 Dynamic Context Notice"));
}

#[test]
fn test_telemetry_prompt_cache_hit_rate() {
    use chilli_context::types::{ContextEngineTelemetry, TokenTelemetry};

    let mut tel = TokenTelemetry::new();
    tel.input_tokens = 200;
    tel.cached_tokens = 800;
    tel.update_total();

    assert_eq!(tel.prompt_cache_hit_rate(), 0.8);
    assert_eq!(tel.total_task_tokens, 200);

    let engine_tel = ContextEngineTelemetry {
        cache_hits: 9,
        cache_misses: 1,
        ..Default::default()
    };

    assert_eq!(engine_tel.cache_hit_rate(), 0.9);
}
