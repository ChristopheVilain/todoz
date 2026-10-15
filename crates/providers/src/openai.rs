use crate::{ClassificationProvider, ClassificationRequest, ClassificationResponse, ProviderError};
use async_trait::async_trait;

pub struct OpenAiProvider {
    pub api_key: String,
    pub model: String,
}

impl OpenAiProvider {
    pub fn new(api_key: String, model: String) -> Self {
        Self { api_key, model }
    }
}

#[async_trait]
impl ClassificationProvider for OpenAiProvider {
    fn name(&self) -> &'static str {
        "openai"
    }

    async fn classify(
        &self,
        _request: ClassificationRequest,
    ) -> Result<ClassificationResponse, ProviderError> {
        // TODO: call the Chat Completions API with response_format/structured
        // output matching ClassificationResponse.
        Err(ProviderError::Request(
            "openai provider not yet implemented".to_string(),
        ))
    }
}
