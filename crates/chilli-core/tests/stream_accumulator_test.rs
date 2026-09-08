use chilli_core::stream_accumulator::StreamAccumulator;
use chilli_model::adapter::{StreamEvent, ToolCallChunk};

#[test]
fn test_accumulates_fragmented_tool_call_json_chunks() {
    let mut acc = StreamAccumulator::new();
    acc.feed(&StreamEvent::ToolCallChunk(ToolCallChunk {
        id: "call_1".to_string(),
        name: "write_file".to_string(),
        arguments_json: "{\"path\": \"src/".to_string(),
        index: None,
    }));
    acc.feed(&StreamEvent::ToolCallChunk(ToolCallChunk {
        id: "call_1".to_string(),
        name: "write_file".to_string(),
        arguments_json: "lib.rs\", \"content\": \"pub fn test() {}\"}".to_string(),
        index: None,
    }));

    let calls = acc.finish_tool_calls().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "write_file");
    assert_eq!(calls[0].arguments["path"], "src/lib.rs");
    assert_eq!(calls[0].arguments["content"], "pub fn test() {}");
}

#[test]
fn test_accumulates_text_and_multiple_tool_calls() {
    let mut acc = StreamAccumulator::new();
    acc.feed(&StreamEvent::TextChunk("I am writing files.".to_string()));
    acc.feed(&StreamEvent::ToolCallChunk(ToolCallChunk {
        id: "call_1".to_string(),
        name: "read_file".to_string(),
        arguments_json: "{\"path\": \"Cargo.toml\"}".to_string(),
        index: None,
    }));
    acc.feed(&StreamEvent::ToolCallChunk(ToolCallChunk {
        id: "call_2".to_string(),
        name: "grep".to_string(),
        arguments_json: "{\"pattern\": \"tokio\"}".to_string(),
        index: None,
    }));

    assert_eq!(acc.accumulated_text(), "I am writing files.");
    let calls = acc.finish_tool_calls().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].id, "call_1");
    assert_eq!(calls[1].id, "call_2");
}

#[test]
fn test_parallel_sse_interleaved_tool_calls_with_index_gaps() {
    let mut acc = StreamAccumulator::new();

    // Chunk for index 1 arrives FIRST (creating index 0 gap padding)
    acc.feed(&StreamEvent::ToolCallChunk(ToolCallChunk {
        id: "call_index_1".to_string(),
        name: "grep".to_string(),
        arguments_json: "{\"pattern\": \"tokio\"}".to_string(),
        index: Some(1),
    }));

    // Chunk for index 0 arrives SECOND
    acc.feed(&StreamEvent::ToolCallChunk(ToolCallChunk {
        id: "call_index_0".to_string(),
        name: "read_file".to_string(),
        arguments_json: "{\"path\": \"Cargo.toml\"}".to_string(),
        index: Some(0),
    }));

    let calls = acc.finish_tool_calls().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].id, "call_index_0");
    assert_eq!(calls[0].name, "read_file");
    assert_eq!(calls[1].id, "call_index_1");
    assert_eq!(calls[1].name, "grep");
}

#[test]
fn test_parallel_sse_unpopulated_gap_filtered() {
    let mut acc = StreamAccumulator::new();

    // Chunk for index 2 arrives (indices 0 and 1 left unpopulated)
    acc.feed(&StreamEvent::ToolCallChunk(ToolCallChunk {
        id: "call_index_2".to_string(),
        name: "exec".to_string(),
        arguments_json: "{\"command\": \"ls\"}".to_string(),
        index: Some(2),
    }));

    let calls = acc.finish_tool_calls().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].id, "call_index_2");
    assert_eq!(calls[0].name, "exec");
}

#[test]
fn test_fuzzy_tool_call_extraction_from_markdown_and_text() {
    let mut acc = StreamAccumulator::new();
    let text = r#"Here is my analysis. I will now edit the file to fix the bug.

```json
{
  "name": "edit_file",
  "arguments": {
    "path": "src/main.rs",
    "old_string": "let x = 1;",
    "new_string": "let x = 2;"
  }
}
```
"#;
    acc.feed(&StreamEvent::TextChunk(text.to_string()));

    let calls = acc.finish_tool_calls().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "edit_file");
    assert_eq!(calls[0].arguments["path"], "src/main.rs");
    assert_eq!(calls[0].arguments["old_string"], "let x = 1;");
    assert_eq!(calls[0].arguments["new_string"], "let x = 2;");
}

#[test]
fn test_fuzzy_tool_call_extraction_with_alternative_key_names() {
    let mut acc = StreamAccumulator::new();
    let text = r#"I will inspect the file using read_file:
{"tool": "read_file", "parameters": {"file_path": "Cargo.toml"}}
"#;
    acc.feed(&StreamEvent::TextChunk(text.to_string()));

    let calls = acc.finish_tool_calls().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "read_file");
    assert_eq!(calls[0].arguments["path"], "Cargo.toml");
}
