use chilli_model::capabilities::{ModelCapabilities, ReasoningTier};
use chilli_model::endpoint::ModelEndpoint;
use chilli_model::manager::ModelManager;
use chilli_model::resilience::ProviderHealth;
use chilli_model::router::{CapabilityRegistry, Requirement};

#[test]
fn test_manager_candidate_selection_and_ranking() {
    let mut registry = CapabilityRegistry::new();

    registry.register(ModelEndpoint {
        provider_name: "groq".to_string(),
        model_id: "llama-3.3-70b".to_string(),
        endpoint_url: "https://api.groq.com/openai/v1".to_string(),
        api_key_env_var: "GROQ_API_KEY".to_string(),
        capabilities: ModelCapabilities {
            max_context_tokens: 128_000,
            max_output_tokens: 8192,
            cost_per_1k_input_usd: 0.00059,
            cost_per_1k_output_usd: 0.00079,
            supports_prompt_caching: false,
            supports_structured_output: true,
            supports_tool_calling: true,
            supports_vision: false,
            supports_streaming: true,
            reasoning_tier: ReasoningTier::Medium,
        },
    });

    registry.register(ModelEndpoint {
        provider_name: "anthropic".to_string(),
        model_id: "claude-3-7-sonnet".to_string(),
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

    let mut manager = ModelManager::new(registry);
    let reqs = vec![Requirement::RequiresToolCalling];
    let candidates = manager.select_candidates(&reqs);

    assert_eq!(candidates.len(), 2);
    // Lower cost provider (groq) comes first
    assert_eq!(candidates[0].provider_name, "groq");
    assert_eq!(candidates[1].provider_name, "anthropic");
}

#[test]
fn test_manager_circuit_breaker_health_tracking() {
    let registry = CapabilityRegistry::with_default_profiles();
    let mut manager = ModelManager::new(registry);

    assert_eq!(
        manager.get_provider_health("anthropic"),
        ProviderHealth::Healthy
    );
    assert_eq!(manager.get_provider_health("groq"), ProviderHealth::Healthy);
}
