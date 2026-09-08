use chilli_model::failure::FailureClass;

#[test]
fn test_failure_classification_retry_and_circuit_rules() {
    let rate_limit = FailureClass::RateLimited { retry_after: None };
    let auth_error = FailureClass::Authentication;
    let server_err = FailureClass::ServerError { status: 503 };

    assert!(rate_limit.is_retryable());
    assert!(rate_limit.should_open_circuit());

    assert!(!auth_error.is_retryable());
    assert!(!auth_error.should_open_circuit());

    assert!(server_err.is_retryable());
    assert!(server_err.should_open_circuit());
}
