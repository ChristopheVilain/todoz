use crate::{ActionItem, ActionItemState};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScorekeeperReport {
    pub done_this_window: usize,
    pub remaining: usize,
    pub todo_next_window: usize,
}

pub fn report(
    items: &[ActionItem],
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    next_window_end: DateTime<Utc>,
) -> ScorekeeperReport {
    let done_this_window = items
        .iter()
        .filter(|item| {
            item.done_at
                .is_some_and(|at| at >= window_start && at < window_end)
        })
        .count();

    let remaining = items
        .iter()
        .filter(|item| matches!(item.state(), ActionItemState::Active | ActionItemState::Backlog))
        .count();

    let todo_next_window = items
        .iter()
        .filter(|item| {
            matches!(item.state(), ActionItemState::Active | ActionItemState::Backlog)
                && item
                    .due_date
                    .is_some_and(|due| due >= window_end && due < next_window_end)
        })
        .count();

    ScorekeeperReport {
        done_this_window,
        remaining,
        todo_next_window,
    }
}
