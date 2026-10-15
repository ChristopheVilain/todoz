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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use uuid::Uuid;

    fn blank_item() -> ActionItem {
        ActionItem {
            id: Uuid::new_v4(),
            note_id: Uuid::new_v4(),
            title: "test item".to_string(),
            effort: None,
            due_date: None,
            created_at: Utc::now(),
            classified_at: None,
            active_at: None,
            backlog_at: None,
            done_at: None,
            dismissed_at: None,
        }
    }

    #[test]
    fn fresh_item_is_inbox() {
        assert_eq!(derive_state(&blank_item()), ActionItemState::Inbox);
    }

    #[test]
    fn active_item_is_active() {
        let mut item = blank_item();
        item.active_at = Some(Utc::now());
        assert_eq!(derive_state(&item), ActionItemState::Active);
    }

    #[test]
    fn backlogged_item_is_backlog() {
        let mut item = blank_item();
        item.active_at = Some(Utc::now() - Duration::days(2));
        item.backlog_at = Some(Utc::now());
        assert_eq!(derive_state(&item), ActionItemState::Backlog);
    }

    #[test]
    fn reactivating_a_backlog_item_returns_to_active() {
        let mut item = blank_item();
        item.backlog_at = Some(Utc::now() - Duration::days(2));
        item.active_at = Some(Utc::now());
        assert_eq!(derive_state(&item), ActionItemState::Active);
    }

    #[test]
    fn done_item_is_done_even_if_backlogged_earlier() {
        let mut item = blank_item();
        item.backlog_at = Some(Utc::now() - Duration::days(2));
        item.done_at = Some(Utc::now());
        assert_eq!(derive_state(&item), ActionItemState::Done);
    }

    #[test]
    fn dismissed_item_is_dismissed_even_after_being_done() {
        // A done item that gets dismissed later (e.g. marked done by mistake)
        // should reflect the more recent dismissal.
        let mut item = blank_item();
        item.done_at = Some(Utc::now() - Duration::days(1));
        item.dismissed_at = Some(Utc::now());
        assert_eq!(derive_state(&item), ActionItemState::Dismissed);
    }
}
