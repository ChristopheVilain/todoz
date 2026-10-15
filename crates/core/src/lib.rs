use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub mod lifecycle;
pub mod scorekeeper;

pub use lifecycle::ActionItemState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: Uuid,
    pub raw_text: String,
    pub status: NoteStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteStatus {
    Editing,
    Idle,
    Classified,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItem {
    pub id: Uuid,
    pub note_id: Uuid,
    pub title: String,
    pub effort: Option<Effort>,
    pub due_date: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub classified_at: Option<DateTime<Utc>>,
    pub active_at: Option<DateTime<Utc>>,
    pub backlog_at: Option<DateTime<Utc>>,
    pub done_at: Option<DateTime<Utc>>,
    pub dismissed_at: Option<DateTime<Utc>>,
}

impl ActionItem {
    pub fn state(&self) -> ActionItemState {
        lifecycle::derive_state(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    Xs,
    S,
    M,
    L,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TagKind {
    General,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TagStatus {
    Active,
    PendingApproval,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: Uuid,
    pub name: String,
    pub kind: TagKind,
    pub status: TagStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionAction {
    Accept,
    Defer,
    Dismiss,
    Merge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub id: Uuid,
    pub item_id: Uuid,
    pub action: DecisionAction,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub idle_timeout_s: u32,
    pub custom_tag_review_threshold: u32,
    pub scorekeeper_window: ScorekeeperWindow,
    pub week_start_day: chrono::Weekday,
    pub active_provider: String,
    pub active_model: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            idle_timeout_s: 60,
            custom_tag_review_threshold: 25,
            scorekeeper_window: ScorekeeperWindow::Week,
            week_start_day: chrono::Weekday::Mon,
            active_provider: "anthropic".to_string(),
            active_model: "claude-sonnet-5".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScorekeeperWindow {
    Day,
    Week,
    Month,
    CustomDays(u32),
}
