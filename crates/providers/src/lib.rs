use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use todoz_core::Effort;

pub mod anthropic;
pub mod mistral;
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
