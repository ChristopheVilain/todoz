use crate::{ClassificationProvider, ClassificationRequest, ClassificationResponse, ProviderError};
use async_trait::async_trait;

pub struct AnthropicProvider {
    pub api_key: String,
    pub model: String,
}

impl AnthropicProvider {
    pub fn new(api_key: String, model: String) -> Self {
        Self { api_key, model }
    }
}

#[async_trait]
impl ClassificationProvider for AnthropicProvider {
    fn name(&self) -> &'static str {
        "anthropic"
    }

    async fn classify(
        &self,
        _request: ClassificationRequest,
    ) -> Result<ClassificationResponse, ProviderError> {
        // TODO: call the Messages API with a tool-use/structured-output schema
        // matching ClassificationResponse.
        Err(ProviderError::Request(
            "anthropic provider not yet implemented".to_string(),
        ))
    }
}
