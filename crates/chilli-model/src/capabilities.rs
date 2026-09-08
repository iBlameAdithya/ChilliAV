use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;

pub static ENABLE_HYSTERESIS: AtomicBool = AtomicBool::new(true);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ReasoningTier {
    Low,
    Medium,
    High,
    Reasoning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
    pub max_context_tokens: usize,
    pub max_output_tokens: usize,
    pub cost_per_1k_input_usd: f64,
    pub cost_per_1k_output_usd: f64,
    pub supports_prompt_caching: bool,
    pub supports_structured_output: bool,
    pub supports_tool_calling: bool,
    pub supports_vision: bool,
    pub supports_streaming: bool,
    pub reasoning_tier: ReasoningTier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilityTier {
    Unknown,
    Micro,
    Small,
    Medium,
    Frontier,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationSample {
    pub tool_call_accuracy: f32,
    pub instruction_following: f32,
    pub verification_success: f32,
    pub recovery_success: f32,
}

impl ObservationSample {
    pub fn composite_score(&self) -> f32 {
        (self.tool_call_accuracy * 0.3)
            + (self.instruction_following * 0.3)
            + (self.verification_success * 0.2)
            + (self.recovery_success * 0.2)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilityProfile {
    pub provider_id: String,
    pub model_id: String,
    pub context_capacity: usize,
    pub window_capacity: usize,
    pub observations: VecDeque<ObservationSample>,
    pub current_tier: CapabilityTier,
    pub consecutive_promotions: usize,
    pub consecutive_demotions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityAssessment {
    pub current_tier: CapabilityTier,
    pub confidence: f32,
    pub is_cold_start: bool,
    pub promotion_threshold: f32,
    pub demotion_threshold: f32,
    pub evidence_window: usize,
}

impl ModelCapabilityProfile {
    pub fn new(provider_id: &str, model_id: &str, context_capacity: usize) -> Self {
        Self {
            provider_id: provider_id.to_string(),
            model_id: model_id.to_string(),
            context_capacity,
            window_capacity: 10,
            observations: VecDeque::new(),
            current_tier: CapabilityTier::Unknown,
            consecutive_promotions: 0,
            consecutive_demotions: 0,
        }
    }

    pub fn record_observation(&mut self, sample: ObservationSample) {
        if self.observations.len() >= self.window_capacity {
            self.observations.pop_front();
        }
        self.observations.push_back(sample);
        self.update_tier_with_hysteresis();
    }

    pub fn weighted_score(&self) -> f32 {
        if self.observations.is_empty() {
            return 0.0;
        }
        let mut total_weight = 0.0;
        let mut weighted_sum = 0.0;
        for (i, sample) in self.observations.iter().enumerate() {
            let weight = 1.0 + (i as f32 * 0.2);
            weighted_sum += sample.composite_score() * weight;
            total_weight += weight;
        }
        weighted_sum / total_weight
    }

    fn update_tier_with_hysteresis(&mut self) {
        let hysteresis_enabled = ENABLE_HYSTERESIS.load(std::sync::atomic::Ordering::SeqCst);
        let evidence_window = if hysteresis_enabled { 3 } else { 1 };
        let score = self.weighted_score();

        let target_tier = match score {
            s if s >= 0.85 => CapabilityTier::Frontier,
            s if s >= 0.70 => CapabilityTier::Medium,
            s if s >= 0.50 => CapabilityTier::Small,
            _ => CapabilityTier::Micro,
        };

        if self.current_tier == CapabilityTier::Unknown {
            if self.observations.len() >= evidence_window {
                self.current_tier = target_tier;
                self.consecutive_promotions = 0;
                self.consecutive_demotions = 0;
            }
            return;
        }

        if target_tier_rank(target_tier) > target_tier_rank(self.current_tier) {
            self.consecutive_promotions += 1;
            self.consecutive_demotions = 0;
            if self.consecutive_promotions >= evidence_window {
                self.current_tier = target_tier;
                self.consecutive_promotions = 0;
            }
        } else if target_tier_rank(target_tier) < target_tier_rank(self.current_tier) {
            self.consecutive_demotions += 1;
            self.consecutive_promotions = 0;
            if self.consecutive_demotions >= evidence_window {
                self.current_tier = target_tier;
                self.consecutive_demotions = 0;
            }
        } else {
            self.consecutive_promotions = 0;
            self.consecutive_demotions = 0;
        }
    }

    pub fn assess(&self) -> CapabilityAssessment {
        let is_cold_start = self.current_tier == CapabilityTier::Unknown;
        let confidence = (self.observations.len() as f32 / self.window_capacity as f32).min(1.0);
        let hysteresis_enabled = ENABLE_HYSTERESIS.load(std::sync::atomic::Ordering::SeqCst);
        let evidence_window = if hysteresis_enabled { 3 } else { 1 };

        CapabilityAssessment {
            current_tier: self.current_tier,
            confidence,
            is_cold_start,
            promotion_threshold: 0.75,
            demotion_threshold: 0.45,
            evidence_window,
        }
    }
}

fn target_tier_rank(tier: CapabilityTier) -> usize {
    match tier {
        CapabilityTier::Unknown => 0,
        CapabilityTier::Micro => 1,
        CapabilityTier::Small => 2,
        CapabilityTier::Medium => 3,
        CapabilityTier::Frontier => 4,
    }
}
