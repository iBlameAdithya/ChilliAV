use crate::failure::FailureClass;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderHealth {
    Healthy,
    Degraded,
    Open,
    Recovering,
}

pub struct CircuitBreaker {
    pub failure_count: u32,
    pub failure_threshold: u32,
    pub state: ProviderHealth,
    pub last_state_change: Instant,
    pub recovery_timeout: Duration,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, recovery_timeout: Duration) -> Self {
        Self {
            failure_count: 0,
            failure_threshold,
            state: ProviderHealth::Healthy,
            last_state_change: Instant::now(),
            recovery_timeout,
        }
    }

    pub fn record_success(&mut self) {
        match self.state {
            ProviderHealth::Recovering => {
                self.state = ProviderHealth::Healthy;
                self.failure_count = 0;
            }
            ProviderHealth::Degraded => {
                self.failure_count = self.failure_count.saturating_sub(1);
                if self.failure_count == 0 {
                    self.state = ProviderHealth::Healthy;
                }
            }
            _ => {}
        }
    }

    pub fn record_failure(&mut self, failure_class: &FailureClass) {
        if !failure_class.should_open_circuit() {
            return;
        }

        self.failure_count += 1;
        self.last_state_change = Instant::now();

        if self.failure_count >= self.failure_threshold {
            self.state = ProviderHealth::Open;
        } else {
            self.state = ProviderHealth::Degraded;
        }
    }

    pub fn check_state(&mut self) -> ProviderHealth {
        if self.state == ProviderHealth::Open
            && self.last_state_change.elapsed() >= self.recovery_timeout
        {
            self.state = ProviderHealth::Recovering;
        }
        self.state
    }
}

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    pub jitter_factor: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(200),
            max_backoff: Duration::from_secs(5),
            jitter_factor: 0.1,
        }
    }
}

impl RetryPolicy {
    pub fn calculate_delay(&self, attempt: u32) -> Duration {
        let exp = 2u64.saturating_pow(attempt.saturating_sub(1));
        let base_millis = self.initial_backoff.as_millis() as u64 * exp;
        Duration::from_millis(base_millis).min(self.max_backoff)
    }
}
