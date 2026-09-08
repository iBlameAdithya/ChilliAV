//! Security sandbox capability detector stub.

#[derive(Debug, Clone)]
pub struct SandboxCapabilities {
    pub is_isolated: bool,
    pub platform_mechanism: String,
}

impl SandboxCapabilities {
    pub fn detect() -> Self {
        Self {
            is_isolated: false,
            platform_mechanism: "unsupported".to_string(),
        }
    }
}
