use crate::capabilities::{ModelCapabilities, ReasoningTier};
use crate::endpoint::ModelEndpoint;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Requirement {
    MinContext(usize),
    RequiresPromptCaching,
    MaxCostPer1k(f64),
    MinReasoningTier(ReasoningTier),
    RequiresToolCalling,
    RequiresVision,
    RequiresStructuredOutput,
}

#[derive(Default, Debug, Clone)]
pub struct CapabilityRegistry {
    endpoints: Vec<ModelEndpoint>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self {
            endpoints: Vec::new(),
        }
    }

    pub fn with_default_profiles() -> Self {
        let default_endpoints = vec![
            ModelEndpoint {
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
                    reasoning_tier: ReasoningTier::Reasoning,
                },
            },
            ModelEndpoint {
                provider_name: "openai".to_string(),
                model_id: "gpt-5.6-sol-1".to_string(),
                endpoint_url: "https://api.openai.com/v1".to_string(),
                api_key_env_var: "OPENAI_API_KEY".to_string(),
                capabilities: ModelCapabilities {
                    max_context_tokens: 400_000,
                    max_output_tokens: 16384,
                    cost_per_1k_input_usd: 0.002,
                    cost_per_1k_output_usd: 0.008,
                    supports_prompt_caching: true,
                    supports_structured_output: true,
                    supports_tool_calling: true,
                    supports_vision: true,
                    supports_streaming: true,
                    reasoning_tier: ReasoningTier::Reasoning,
                },
            },
            ModelEndpoint {
                provider_name: "openai".to_string(),
                model_id: "gpt-4o".to_string(),
                endpoint_url: "https://api.openai.com/v1".to_string(),
                api_key_env_var: "OPENAI_API_KEY".to_string(),
                capabilities: ModelCapabilities {
                    max_context_tokens: 128_000,
                    max_output_tokens: 4096,
                    cost_per_1k_input_usd: 0.0025,
                    cost_per_1k_output_usd: 0.01,
                    supports_prompt_caching: true,
                    supports_structured_output: true,
                    supports_tool_calling: true,
                    supports_vision: true,
                    supports_streaming: true,
                    reasoning_tier: ReasoningTier::High,
                },
            },
            ModelEndpoint {
                provider_name: "groq".to_string(),
                model_id: "llama-3.3-70b-versatile".to_string(),
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
            },
            ModelEndpoint {
                provider_name: "gemini".to_string(),
                model_id: "gemini-3.6-flash".to_string(),
                endpoint_url: "https://generativelanguage.googleapis.com/v1beta/openai".to_string(),
                api_key_env_var: "GEMINI_API_KEY".to_string(),
                capabilities: ModelCapabilities {
                    max_context_tokens: 1_000_000,
                    max_output_tokens: 8192,
                    cost_per_1k_input_usd: 0.0001,
                    cost_per_1k_output_usd: 0.0004,
                    supports_prompt_caching: true,
                    supports_structured_output: true,
                    supports_tool_calling: true,
                    supports_vision: true,
                    supports_streaming: true,
                    reasoning_tier: ReasoningTier::High,
                },
            },
            ModelEndpoint {
                provider_name: "agcl".to_string(),
                model_id: "gemini-3.6-flash-medium".to_string(),
                endpoint_url: "http://localhost:8080/v1".to_string(),
                api_key_env_var: "AGCL_API_KEY".to_string(),
                capabilities: ModelCapabilities {
                    max_context_tokens: 300_000,
                    max_output_tokens: 8192,
                    cost_per_1k_input_usd: 0.0,
                    cost_per_1k_output_usd: 0.0,
                    supports_prompt_caching: true,
                    supports_structured_output: true,
                    supports_tool_calling: true,
                    supports_vision: true,
                    supports_streaming: true,
                    reasoning_tier: ReasoningTier::High,
                },
            },
        ];
        Self {
            endpoints: default_endpoints,
        }
    }

    pub fn register(&mut self, endpoint: ModelEndpoint) {
        self.endpoints.push(endpoint);
    }

    pub fn load_from_json(&mut self, json_str: &str) -> Result<(), String> {
        let loaded: Vec<ModelEndpoint> = serde_json::from_str(json_str)
            .map_err(|e| format!("Failed to parse model endpoints JSON: {}", e))?;
        self.endpoints.extend(loaded);
        Ok(())
    }

    pub fn select(&self, reqs: &[Requirement]) -> Result<ModelEndpoint, String> {
        let candidates: Vec<&ModelEndpoint> = self
            .endpoints
            .iter()
            .filter(|e| {
                reqs.iter().all(|req| match req {
                    Requirement::MinContext(min) => e.capabilities.max_context_tokens >= *min,
                    Requirement::RequiresPromptCaching => e.capabilities.supports_prompt_caching,
                    Requirement::MaxCostPer1k(max_cost) => {
                        e.capabilities.cost_per_1k_input_usd <= *max_cost
                    }
                    Requirement::MinReasoningTier(tier) => e.capabilities.reasoning_tier >= *tier,
                    Requirement::RequiresToolCalling => e.capabilities.supports_tool_calling,
                    Requirement::RequiresVision => e.capabilities.supports_vision,
                    Requirement::RequiresStructuredOutput => {
                        e.capabilities.supports_structured_output
                    }
                })
            })
            .collect();

        if candidates.is_empty() {
            return Err("No model matching required capabilities found".to_string());
        }

        let mut sorted = candidates;
        sorted.sort_by(|a, b| {
            a.capabilities
                .cost_per_1k_input_usd
                .partial_cmp(&b.capabilities.cost_per_1k_input_usd)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    b.capabilities
                        .max_context_tokens
                        .cmp(&a.capabilities.max_context_tokens)
                })
        });

        Ok((*sorted[0]).clone())
    }

    pub fn endpoints(&self) -> &[ModelEndpoint] {
        &self.endpoints
    }
}
