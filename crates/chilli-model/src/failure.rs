use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureClass {
    RateLimited { retry_after: Option<Duration> },
    Transient,
    ServerError { status: u16 },
    Timeout,
    Authentication,
    InvalidRequest,
    ContextExceeded,
    UnsupportedCapability,
    StreamInterrupted,
    Unknown,
}

impl FailureClass {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            FailureClass::RateLimited { .. }
                | FailureClass::Transient
                | FailureClass::ServerError { .. }
                | FailureClass::Timeout
                | FailureClass::StreamInterrupted
        )
    }

    pub fn should_open_circuit(&self) -> bool {
        matches!(
            self,
            FailureClass::RateLimited { .. }
                | FailureClass::ServerError { .. }
                | FailureClass::Timeout
        )
    }
}
