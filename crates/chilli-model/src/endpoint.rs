use crate::capabilities::ModelCapabilities;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEndpoint {
    pub provider_name: String,
    pub model_id: String,
    pub endpoint_url: String,
    pub api_key_env_var: String,
    pub capabilities: ModelCapabilities,
}
