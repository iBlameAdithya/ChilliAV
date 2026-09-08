use crate::adapter::{ModelAdapter, StreamEvent};
use async_trait::async_trait;

pub struct OpenAICompatibleAdapter {
    provider_name: String,
    openai_adapter: crate::openai::OpenAIAdapter,
}

impl OpenAICompatibleAdapter {
    pub fn new(provider: impl Into<String>) -> Self {
        let provider_name = provider.into();
        Self {
            provider_name,
            openai_adapter: crate::openai::OpenAIAdapter::from_env(),
        }
    }
}

#[async_trait]
impl ModelAdapter for OpenAICompatibleAdapter {
    fn provider_name(&self) -> &'static str {
        Box::leak(self.provider_name.clone().into_boxed_str())
    }

    fn parse_sse_data(&self, json_data: &str) -> Result<Vec<StreamEvent>, String> {
        self.openai_adapter.parse_sse_data(json_data)
    }
}
