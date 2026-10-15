use crate::ActionItem;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionItemState {
    Inbox,
    Active,
    Backlog,
    Done,
    Dismissed,
}

/// State is derived from whichever lifecycle timestamp is most recent, so
/// reactivating a backlogged item (a fresh active_at) correctly overrides an
/// older backlog_at instead of requiring a separate mutable status field.
pub fn derive_state(item: &ActionItem) -> ActionItemState {
    [
        (item.dismissed_at, ActionItemState::Dismissed),
        (item.done_at, ActionItemState::Done),
        (item.backlog_at, ActionItemState::Backlog),
        (item.active_at, ActionItemState::Active),
    ]
    .into_iter()
    .filter_map(|(at, state)| at.map(|at| (at, state)))
    .max_by_key(|(at, _)| *at)
    .map(|(_, state)| state)
    .unwrap_or(ActionItemState::Inbox)
}
