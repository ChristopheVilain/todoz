use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use todoz_core::Effort;

pub mod anthropic;
pub mod mistral;
pub mod mock;
pub mod openai;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationRequest {
    pub note_text: String,
    pub existing_general_tags: Vec<String>,
    pub existing_custom_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationResponse {
    pub action_items: Vec<DraftActionItem>,
    pub is_actionable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftActionItem {
    pub title: String,
    pub effort: Option<Effort>,
    pub due_date: Option<chrono::DateTime<chrono::Utc>>,
    pub general_tags: Vec<String>,
    /// Tag names the model proposes; may or may not already exist. Whether a
    /// genuinely new name auto-applies or queues for approval is a storage/
    /// settings-layer decision (custom_tag_review_threshold), not this trait's.
    pub custom_tags: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("provider request failed: {0}")]
    Request(String),
    #[error("provider returned an unparseable response: {0}")]
    MalformedResponse(String),
    #[error("provider authentication failed")]
    Unauthorized,
}

#[async_trait]
pub trait ClassificationProvider: Send + Sync {
    fn name(&self) -> &'static str;

    async fn classify(
        &self,
        request: ClassificationRequest,
    ) -> Result<ClassificationResponse, ProviderError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anthropic::AnthropicProvider;
    use crate::mistral::MistralProvider;
    use crate::openai::OpenAiProvider;

    fn sample_request() -> ClassificationRequest {
        ClassificationRequest {
            note_text: "call the vendor back about the Q3 invoice".to_string(),
            existing_general_tags: vec!["finance".to_string()],
            existing_custom_tags: vec!["Q3 Budget Review".to_string()],
        }
    }

    #[test]
    fn classification_response_round_trips_through_json() {
        let response = ClassificationResponse {
            is_actionable: true,
            action_items: vec![DraftActionItem {
                title: "call the vendor back".to_string(),
                effort: Some(Effort::S),
                due_date: None,
                general_tags: vec!["finance".to_string()],
                custom_tags: vec!["Q3 Budget Review".to_string()],
            }],
        };

        let json = serde_json::to_string(&response).expect("serialize");
        let parsed: ClassificationResponse = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(parsed.is_actionable, response.is_actionable);
        assert_eq!(parsed.action_items.len(), 1);
        assert_eq!(parsed.action_items[0].title, "call the vendor back");
        assert_eq!(parsed.action_items[0].effort, Some(Effort::S));
    }

    #[tokio::test]
    async fn anthropic_stub_reports_not_implemented_rather_than_succeeding() {
        let provider = AnthropicProvider::new("test-key".to_string(), "claude-sonnet-5".to_string());
        assert_eq!(provider.name(), "anthropic");
        let result = provider.classify(sample_request()).await;
        assert!(result.is_err(), "stub provider must not silently succeed");
    }

    #[tokio::test]
    async fn openai_stub_reports_not_implemented_rather_than_succeeding() {
        let provider = OpenAiProvider::new("test-key".to_string(), "gpt-5".to_string());
        assert_eq!(provider.name(), "openai");
        let result = provider.classify(sample_request()).await;
        assert!(result.is_err(), "stub provider must not silently succeed");
    }

    #[tokio::test]
    async fn mistral_stub_reports_not_implemented_rather_than_succeeding() {
        let provider = MistralProvider::new("test-key".to_string(), "mistral-large".to_string());
        assert_eq!(provider.name(), "mistral");
        let result = provider.classify(sample_request()).await;
        assert!(result.is_err(), "stub provider must not silently succeed");
    }
}
