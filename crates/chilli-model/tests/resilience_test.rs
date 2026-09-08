use chilli_model::failure::FailureClass;
use chilli_model::resilience::{CircuitBreaker, ProviderHealth, RetryPolicy};
use std::time::Duration;

#[test]
fn test_circuit_breaker_state_transitions() {
    let mut cb = CircuitBreaker::new(2, Duration::from_millis(100));
    assert_eq!(cb.check_state(), ProviderHealth::Healthy);

    let err = FailureClass::ServerError { status: 500 };
    cb.record_failure(&err);
    assert_eq!(cb.check_state(), ProviderHealth::Degraded);

    cb.record_failure(&err);
    assert_eq!(cb.check_state(), ProviderHealth::Open);

    // Sleep past recovery timeout
    std::thread::sleep(Duration::from_millis(110));
    assert_eq!(cb.check_state(), ProviderHealth::Recovering);

    cb.record_success();
    assert_eq!(cb.check_state(), ProviderHealth::Healthy);
}

#[test]
fn test_retry_policy_exponential_backoff() {
    let policy = RetryPolicy::default();
    let delay0 = policy.calculate_delay(1);
    let delay1 = policy.calculate_delay(2);
    assert!(delay1 >= delay0);
}
