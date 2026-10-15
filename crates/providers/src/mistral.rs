use crate::{ClassificationProvider, ClassificationRequest, ClassificationResponse, ProviderError};
use async_trait::async_trait;

pub struct MistralProvider {
    pub api_key: String,
    pub model: String,
}

impl MistralProvider {
    pub fn new(api_key: String, model: String) -> Self {
        Self { api_key, model }
    }
}

#[async_trait]
impl ClassificationProvider for MistralProvider {
    fn name(&self) -> &'static str {
        "mistral"
    }

    async fn classify(
        &self,
        _request: ClassificationRequest,
    ) -> Result<ClassificationResponse, ProviderError> {
        // TODO: Mistral's chat completions API is OpenAI-compatible, so this
        // can likely share most of openai.rs's request/response handling.
        Err(ProviderError::Request(
            "mistral provider not yet implemented".to_string(),
        ))
    }
}
