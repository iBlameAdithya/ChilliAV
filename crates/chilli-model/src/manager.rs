use crate::adapter::{
    CompletionRequest, ModelAdapter, ModelResponse, StreamAccumulator, TokenUsage,
};
use crate::anthropic::AnthropicAdapter;
use crate::endpoint::ModelEndpoint;
use crate::failure::FailureClass;
use crate::openai::OpenAIAdapter;
use crate::openai_compatible::OpenAICompatibleAdapter;
use crate::resilience::{CircuitBreaker, ProviderHealth, RetryPolicy};
use crate::router::{CapabilityRegistry, Requirement};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct ModelExecutionReport {
    pub endpoint: ModelEndpoint,
    pub response: ModelResponse,
    pub attempts: u32,
    pub latency_ms: u64,
    pub token_usage: TokenUsage,
    pub cost_usd: f64,
}

pub struct ModelManager {
    registry: CapabilityRegistry,
    circuit_breakers: HashMap<String, CircuitBreaker>,
    retry_policy: RetryPolicy,
    failure_threshold: u32,
    recovery_timeout: Duration,
}

impl ModelManager {
    pub fn new(registry: CapabilityRegistry) -> Self {
        Self {
            registry,
            circuit_breakers: HashMap::new(),
            retry_policy: RetryPolicy::default(),
            failure_threshold: 3,
            recovery_timeout: Duration::from_secs(30),
        }
    }

    pub fn with_retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = policy;
        self
    }

    pub fn registry(&self) -> &CapabilityRegistry {
        &self.registry
    }

    pub fn registry_mut(&mut self) -> &mut CapabilityRegistry {
        &mut self.registry
    }

    pub fn get_provider_health(&mut self, provider_name: &str) -> ProviderHealth {
        if let Some(cb) = self.circuit_breakers.get_mut(provider_name) {
            cb.check_state()
        } else {
            ProviderHealth::Healthy
        }
    }

    fn get_adapter(&self, endpoint: &ModelEndpoint) -> Arc<dyn ModelAdapter> {
        match endpoint.provider_name.to_lowercase().as_str() {
            "anthropic" | "agcl" => Arc::new(AnthropicAdapter::from_env_with_model(Some(
                &endpoint.model_id,
            ))),
            "openai" | "groq" | "gemini" => {
                Arc::new(OpenAIAdapter::from_env_with_model(Some(&endpoint.model_id)))
            }
            _ => Arc::new(OpenAICompatibleAdapter::new(&endpoint.provider_name)),
        }
    }

    pub fn select_candidates(&mut self, requirements: &[Requirement]) -> Vec<ModelEndpoint> {
        let endpoints = self.registry.endpoints();
        let mut candidates: Vec<ModelEndpoint> = endpoints
            .iter()
            .filter(|e| {
                requirements.iter().all(|req| match req {
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
            .cloned()
            .collect();

        // Sort candidates: lowest input cost first, then largest max context
        candidates.sort_by(|a, b| {
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

        candidates
    }

    pub async fn execute_request(
        &mut self,
        request: &CompletionRequest,
        requirements: &[Requirement],
    ) -> Result<ModelExecutionReport, String> {
        let candidates = self.select_candidates(requirements);
        if candidates.is_empty() {
            return Err("No candidate model endpoint matching requirements found".to_string());
        }

        let mut last_error = String::new();

        for endpoint in candidates {
            // Check circuit breaker status
            let health = self.get_provider_health(&endpoint.provider_name);
            if health == ProviderHealth::Open {
                last_error = format!(
                    "Circuit breaker open for provider: {}",
                    endpoint.provider_name
                );
                continue;
            }

            let adapter = self.get_adapter(&endpoint);
            let start_time = Instant::now();
            let mut attempts = 0u32;

            loop {
                attempts += 1;
                let stream_res = adapter.complete(request.clone()).await;

                match stream_res {
                    Ok(stream) => {
                        use tokio_stream::StreamExt;
                        let mut accumulator = StreamAccumulator::default();
                        let mut pinned_stream = Box::pin(stream);

                        while let Some(event) = pinned_stream.next().await {
                            accumulator.process(event);
                        }

                        let response = accumulator.into_response();
                        let latency_ms = start_time.elapsed().as_millis() as u64;

                        // Record success on circuit breaker
                        let threshold = self.failure_threshold;
                        let timeout = self.recovery_timeout;
                        let cb = self
                            .circuit_breakers
                            .entry(endpoint.provider_name.clone())
                            .or_insert_with(|| CircuitBreaker::new(threshold, timeout));
                        cb.record_success();

                        let input_cost = (response.token_usage.prompt_tokens as f64 / 1000.0)
                            * endpoint.capabilities.cost_per_1k_input_usd;
                        let output_cost = (response.token_usage.completion_tokens as f64 / 1000.0)
                            * endpoint.capabilities.cost_per_1k_output_usd;
                        let cost_usd = input_cost + output_cost;
                        let token_usage = response.token_usage.clone();

                        return Ok(ModelExecutionReport {
                            endpoint,
                            response,
                            attempts,
                            latency_ms,
                            token_usage,
                            cost_usd,
                        });
                    }
                    Err(err_msg) => {
                        let failure_class =
                            if err_msg.contains("rate limit") || err_msg.contains("429") {
                                FailureClass::RateLimited { retry_after: None }
                            } else if err_msg.contains("timeout") || err_msg.contains("timed out") {
                                FailureClass::Timeout
                            } else if err_msg.contains("500")
                                || err_msg.contains("503")
                                || err_msg.contains("server error")
                            {
                                FailureClass::ServerError { status: 500 }
                            } else {
                                FailureClass::Transient
                            };

                        // Record failure on circuit breaker
                        let threshold = self.failure_threshold;
                        let timeout = self.recovery_timeout;
                        let cb = self
                            .circuit_breakers
                            .entry(endpoint.provider_name.clone())
                            .or_insert_with(|| CircuitBreaker::new(threshold, timeout));
                        cb.record_failure(&failure_class);

                        if failure_class.is_retryable() && attempts < self.retry_policy.max_attempts
                        {
                            let delay = self.retry_policy.calculate_delay(attempts);
                            tokio::time::sleep(delay).await;
                            continue;
                        }

                        last_error = format!(
                            "Endpoint {} ({}) failed after {} attempts: {}",
                            endpoint.model_id, endpoint.provider_name, attempts, err_msg
                        );
                        break;
                    }
                }
            }
        }

        Err(format!(
            "All candidate endpoints failed. Last error: {}",
            last_error
        ))
    }
}
