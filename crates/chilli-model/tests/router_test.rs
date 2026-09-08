use chilli_model::adapter::{ModelAdapter, StreamEvent};
use chilli_model::anthropic::AnthropicAdapter;
use chilli_model::capabilities::{ModelCapabilities, ReasoningTier};
use chilli_model::endpoint::ModelEndpoint;
use chilli_model::openai::OpenAIAdapter;
use chilli_model::router::{CapabilityRegistry, Requirement};

#[test]
fn test_capability_routing_and_provider_selection() {
    let mut registry = CapabilityRegistry::new();

    registry.register(ModelEndpoint {
        provider_name: "anthropic".to_string(),
        model_id: "claude-sonnet-capability-test".to_string(),
        endpoint_url: "https://api.anthropic.com/v1".to_string(),
        api_key_env_var: "ANTHROPIC_API_KEY".to_string(),
        capabilities: ModelCapabilities {
            max_context_tokens: 200_000,
            max_output_tokens: 8192,
            cost_per_1k_input_usd: 0.003,
            cost_per_1k_output_usd: 0.015,
            supports_prompt_caching: true,
            supports_structured_output: true,
            supports_tool_calling: true,
            supports_vision: true,
            supports_streaming: true,
            reasoning_tier: ReasoningTier::High,
        },
    });

    registry.register(ModelEndpoint {
        provider_name: "openai".to_string(),
        model_id: "gpt-4o-capability-test".to_string(),
        endpoint_url: "https://api.openai.com/v1".to_string(),
        api_key_env_var: "OPENAI_API_KEY".to_string(),
        capabilities: ModelCapabilities {
            max_context_tokens: 128_000,
            max_output_tokens: 4096,
            cost_per_1k_input_usd: 0.005,
            cost_per_1k_output_usd: 0.015,
            supports_prompt_caching: true,
            supports_structured_output: true,
            supports_tool_calling: true,
            supports_vision: true,
            supports_streaming: true,
            reasoning_tier: ReasoningTier::High,
        },
    });

    let selected = registry
        .select(&[
            Requirement::MinContext(150_000),
            Requirement::RequiresPromptCaching,
        ])
        .unwrap();

    assert_eq!(selected.model_id, "claude-sonnet-capability-test");
}

#[test]
fn test_anthropic_sse_chunk_parsing() {
    let adapter = AnthropicAdapter::default();
    let sse_line = r#"{"type": "content_block_delta", "delta": {"type": "text_delta", "text": "Hello 🌶️ Chilli!"}}"#;

    let events = adapter.parse_sse_data(sse_line).unwrap();
    assert_eq!(events.len(), 1);
    match &events[0] {
        StreamEvent::TextChunk(text) => assert_eq!(text, "Hello 🌶️ Chilli!"),
        _ => panic!("Expected TextChunk"),
    }
}

#[test]
fn test_openai_sse_chunk_parsing() {
    let adapter = OpenAIAdapter::new(
        "dummy_key".to_string(),
        "gpt-4o".to_string(),
        "https://api.openai.com/v1".to_string(),
    );
    let sse_line =
        r#"{"choices": [{"delta": {"content": "Chunk from OpenAI"}, "finish_reason": null}]}"#;

    let events = adapter.parse_sse_data(sse_line).unwrap();
    assert_eq!(events.len(), 1);
    match &events[0] {
        StreamEvent::TextChunk(text) => assert_eq!(text, "Chunk from OpenAI"),
        _ => panic!("Expected TextChunk"),
    }
}

#[test]
fn test_load_capabilities_from_json() {
    let mut registry = CapabilityRegistry::new();
    let json_str = r#"[
        {
            "provider_name": "local",
            "model_id": "ollama-llama3",
            "endpoint_url": "http://localhost:11434/v1",
            "api_key_env_var": "NONE",
            "capabilities": {
                "max_context_tokens": 8192,
                "max_output_tokens": 2048,
                "cost_per_1k_input_usd": 0.0,
                "cost_per_1k_output_usd": 0.0,
                "supports_prompt_caching": false,
                "supports_structured_output": true,
                "supports_tool_calling": true,
                "supports_vision": false,
                "supports_streaming": true,
                "reasoning_tier": "Low"
            }
        }
    ]"#;

    registry.load_from_json(json_str).unwrap();

    let selected = registry
        .select(&[
            Requirement::MinReasoningTier(ReasoningTier::Low),
            Requirement::MaxCostPer1k(0.0),
        ])
        .unwrap();

    assert_eq!(selected.model_id, "ollama-llama3");
    assert_eq!(selected.provider_name, "local");
}
