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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use uuid::Uuid;

    fn item() -> ActionItem {
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
    fn counts_done_items_only_within_the_window() {
        let now = Utc::now();
        let window_start = now - Duration::days(7);
        let window_end = now;
        let next_window_end = now + Duration::days(7);

        let mut done_in_window = item();
        done_in_window.done_at = Some(now - Duration::days(1));

        let mut done_before_window = item();
        done_before_window.done_at = Some(now - Duration::days(10));

        let items = vec![done_in_window, done_before_window];
        let report = report(&items, window_start, window_end, next_window_end);

        assert_eq!(report.done_this_window, 1);
    }

    #[test]
    fn remaining_counts_active_and_backlog_but_not_done_or_dismissed() {
        let now = Utc::now();

        let mut active = item();
        active.active_at = Some(now);

        let mut backlog = item();
        backlog.backlog_at = Some(now);

        let mut done = item();
        done.done_at = Some(now);

        let mut dismissed = item();
        dismissed.dismissed_at = Some(now);

        let items = vec![active, backlog, done, dismissed];
        let report = report(&items, now - Duration::days(7), now, now + Duration::days(7));

        assert_eq!(report.remaining, 2);
    }

    #[test]
    fn todo_next_window_requires_due_date_in_the_upcoming_window() {
        let now = Utc::now();
        let window_end = now;
        let next_window_end = now + Duration::days(7);

        let mut due_next_window = item();
        due_next_window.active_at = Some(now);
        due_next_window.due_date = Some(now + Duration::days(3));

        let mut due_later = item();
        due_later.active_at = Some(now);
        due_later.due_date = Some(now + Duration::days(14));

        let mut no_due_date = item();
        no_due_date.active_at = Some(now);

        let items = vec![due_next_window, due_later, no_due_date];
        let report = report(&items, now - Duration::days(7), window_end, next_window_end);

        assert_eq!(report.todo_next_window, 1);
    }
}
