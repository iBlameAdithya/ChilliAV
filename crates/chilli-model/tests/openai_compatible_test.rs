use chilli_model::adapter::{ModelAdapter, StreamAccumulator, StreamEvent, ToolCallChunk};
use chilli_model::openai_compatible::OpenAICompatibleAdapter;

#[test]
fn test_stream_accumulator_builds_full_response() {
    let mut acc = StreamAccumulator::default();
    acc.process(StreamEvent::TextChunk("Hello ".to_string()));
    acc.process(StreamEvent::TextChunk("World!".to_string()));
    acc.process(StreamEvent::ToolCallChunk(ToolCallChunk {
        id: "call_1".to_string(),
        name: "read_file".to_string(),
        arguments_json: "{\"path\": \"test.rs\"}".to_string(),
        index: None,
    }));

    let resp = acc.into_response();
    assert_eq!(resp.text_content, "Hello World!");
    assert_eq!(resp.tool_calls.len(), 1);
    assert_eq!(resp.tool_calls[0].name, "read_file");
}

#[test]
fn test_openai_compatible_adapter_creation() {
    let adapter = OpenAICompatibleAdapter::new("groq");
    assert_eq!(adapter.provider_name(), "groq");
}

#[test]
fn test_from_env_with_model_prefix_routing() {
    use chilli_model::openai::OpenAIAdapter;

    std::env::set_var("GROQ_API_KEY", "gsk_dummy");

    // GPT model override with GROQ_API_KEY set must direct to OpenAI
    let gpt_adapter = OpenAIAdapter::from_env_with_model(Some("gpt-4o"));
    assert_eq!(gpt_adapter.base_url, "https://api.openai.com/v1");

    // Llama model override direct to Groq
    let llama_adapter = OpenAIAdapter::from_env_with_model(Some("llama-3.3-70b-versatile"));
    assert_eq!(llama_adapter.base_url, "https://api.groq.com/openai/v1");
    assert_eq!(llama_adapter.model_name, "llama-3.3-70b-versatile");

    // Gemini model override direct to Gemini
    let gemini_adapter = OpenAIAdapter::from_env_with_model(Some("gemini-1.5-pro"));
    assert_eq!(
        gemini_adapter.base_url,
        "https://generativelanguage.googleapis.com/v1beta/openai"
    );

    std::env::remove_var("GROQ_API_KEY");
}
