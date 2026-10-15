use chrono::{DateTime, Utc};
use rusqlite::Connection;
use std::path::Path;
use todoz_core::{ActionItem, Decision, DecisionAction, Effort, Note, NoteStatus, Tag, TagKind, TagStatus};
use uuid::Uuid;

const MIGRATIONS: &[&str] = &[include_str!("../migrations/0001_init.sql")];

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error("invalid stored value: {0}")]
    InvalidData(String),
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self, StorageError> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<(), StorageError> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY)",
        )?;
        for (i, migration) in MIGRATIONS.iter().enumerate() {
            let version = i as i64 + 1;
            let already_applied: bool = self.conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
                [version],
                |row| row.get(0),
            )?;
            if already_applied {
                continue;
            }
            self.conn.execute_batch(migration)?;
            self.conn.execute(
                "INSERT INTO schema_migrations (version) VALUES (?1)",
                [version],
            )?;
        }
        Ok(())
    }

    pub fn insert_note(&self, note: &Note) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO notes (id, raw_text, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                note.id.to_string(),
                note.raw_text,
                status_to_str(note.status),
                note.created_at.to_rfc3339(),
                note.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn get_note(&self, id: Uuid) -> Result<Option<Note>, StorageError> {
        self.conn
            .query_row(
                "SELECT id, raw_text, status, created_at, updated_at
                 FROM notes WHERE id = ?1 AND deleted_at IS NULL",
                [id.to_string()],
                row_to_note,
            )
            .map(Some)
            .or_else(|err| match err {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other.into()),
            })
    }

    pub fn update_note(
        &self,
        id: Uuid,
        raw_text: &str,
        status: NoteStatus,
        updated_at: DateTime<Utc>,
    ) -> Result<(), StorageError> {
        self.conn.execute(
            "UPDATE notes SET raw_text = ?1, status = ?2, updated_at = ?3
             WHERE id = ?4 AND deleted_at IS NULL",
            rusqlite::params![
                raw_text,
                status_to_str(status),
                updated_at.to_rfc3339(),
                id.to_string(),
            ],
        )?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, StorageError> {
        self.conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                [key],
                |row| row.get(0),
            )
            .map(Some)
            .or_else(|err| match err {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other.into()),
            })
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }

    pub fn list_notes(&self) -> Result<Vec<Note>, StorageError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, raw_text, status, created_at, updated_at
             FROM notes WHERE deleted_at IS NULL ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], row_to_note)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn insert_action_item(&self, item: &ActionItem) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO action_items
                (id, note_id, title, effort, due_date, created_at, classified_at,
                 active_at, backlog_at, done_at, dismissed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                item.id.to_string(),
                item.note_id.to_string(),
                item.title,
                item.effort.map(effort_to_str),
                item.due_date.map(|d| d.to_rfc3339()),
                item.created_at.to_rfc3339(),
                item.classified_at.map(|d| d.to_rfc3339()),
                item.active_at.map(|d| d.to_rfc3339()),
                item.backlog_at.map(|d| d.to_rfc3339()),
                item.done_at.map(|d| d.to_rfc3339()),
                item.dismissed_at.map(|d| d.to_rfc3339()),
            ],
        )?;
        Ok(())
    }

    pub fn list_items_for_note(&self, note_id: Uuid) -> Result<Vec<ActionItem>, StorageError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, note_id, title, effort, due_date, created_at, classified_at,
                    active_at, backlog_at, done_at, dismissed_at
             FROM action_items WHERE note_id = ?1 ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([note_id.to_string()], row_to_action_item)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get_action_item(&self, id: Uuid) -> Result<Option<ActionItem>, StorageError> {
        self.conn
            .query_row(
                "SELECT id, note_id, title, effort, due_date, created_at, classified_at,
                        active_at, backlog_at, done_at, dismissed_at
                 FROM action_items WHERE id = ?1",
                [id.to_string()],
                row_to_action_item,
            )
            .map(Some)
            .or_else(|err| match err {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other.into()),
            })
    }

    pub fn delete_action_item(&self, id: Uuid) -> Result<(), StorageError> {
        self.conn.execute(
            "DELETE FROM item_tags WHERE item_id = ?1",
            [id.to_string()],
        )?;
        self.conn.execute(
            "DELETE FROM action_items WHERE id = ?1",
            [id.to_string()],
        )?;
        Ok(())
    }

    pub fn set_action_item_timestamp(
        &self,
        id: Uuid,
        field: &str,
        at: DateTime<Utc>,
    ) -> Result<(), StorageError> {
        let column = match field {
            "active_at" | "backlog_at" | "done_at" | "dismissed_at" => field,
            other => {
                return Err(StorageError::InvalidData(format!(
                    "unknown action item timestamp field: {other}"
                )))
            }
        };
        self.conn.execute(
            &format!("UPDATE action_items SET {column} = ?1 WHERE id = ?2"),
            rusqlite::params![at.to_rfc3339(), id.to_string()],
        )?;
        Ok(())
    }

    pub fn get_or_create_tag(&self, name: &str, kind: TagKind) -> Result<Tag, StorageError> {
        if let Some(tag) = self.find_tag_by_name(name)? {
            return Ok(tag);
        }
        let tag = Tag {
            id: Uuid::new_v4(),
            name: name.to_string(),
            kind,
            status: TagStatus::Active,
            created_at: Utc::now(),
        };
        let inserted = self.conn.execute(
            "INSERT INTO tags (id, name, kind, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(name) DO NOTHING",
            rusqlite::params![
                tag.id.to_string(),
                tag.name,
                tag_kind_to_str(tag.kind),
                tag_status_to_str(tag.status),
                tag.created_at.to_rfc3339(),
            ],
        )?;
        if inserted == 0 {
            // Lost a race with a concurrent get_or_create_tag call for the same name.
            return self
                .find_tag_by_name(name)?
                .ok_or_else(|| StorageError::InvalidData(format!("tag {name} vanished")));
        }
        Ok(tag)
    }

    fn find_tag_by_name(&self, name: &str) -> Result<Option<Tag>, StorageError> {
        self.conn
            .query_row(
                "SELECT id, name, kind, status, created_at FROM tags WHERE name = ?1",
                [name],
                row_to_tag,
            )
            .map(Some)
            .or_else(|err| match err {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other.into()),
            })
    }

    pub fn insert_item_tag(&self, item_id: Uuid, tag_id: Uuid) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO item_tags (item_id, tag_id, applied_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(item_id, tag_id) DO NOTHING",
            rusqlite::params![item_id.to_string(), tag_id.to_string(), Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn list_tags_for_item(&self, item_id: Uuid) -> Result<Vec<Tag>, StorageError> {
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.name, t.kind, t.status, t.created_at
             FROM tags t JOIN item_tags it ON it.tag_id = t.id
             WHERE it.item_id = ?1 ORDER BY t.name ASC",
        )?;
        let rows = stmt.query_map([item_id.to_string()], row_to_tag)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Card count = distinct notes having at least one action item carrying the tag.
    pub fn list_tags_with_card_counts(&self) -> Result<Vec<(Tag, i64)>, StorageError> {
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.name, t.kind, t.status, t.created_at,
                    COUNT(DISTINCT ai.note_id)
             FROM tags t
             LEFT JOIN item_tags it ON it.tag_id = t.id
             LEFT JOIN action_items ai ON ai.id = it.item_id
             GROUP BY t.id
             ORDER BY t.name ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            let tag = row_to_tag(row)?;
            let count: i64 = row.get(5)?;
            Ok((tag, count))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn insert_decision(&self, decision: &Decision) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO decisions (id, item_id, action, at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                decision.id.to_string(),
                decision.item_id.to_string(),
                decision_action_to_str(decision.action),
                decision.at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }
}

fn status_to_str(status: NoteStatus) -> &'static str {
    match status {
        NoteStatus::Editing => "editing",
        NoteStatus::Idle => "idle",
        NoteStatus::Classified => "classified",
    }
}

fn str_to_status(s: &str) -> Result<NoteStatus, StorageError> {
    match s {
        "editing" => Ok(NoteStatus::Editing),
        "idle" => Ok(NoteStatus::Idle),
        "classified" => Ok(NoteStatus::Classified),
        other => Err(StorageError::InvalidData(format!(
            "unknown note status: {other}"
        ))),
    }
}

fn row_to_note(row: &rusqlite::Row) -> rusqlite::Result<Note> {
    let id: String = row.get(0)?;
    let raw_text: String = row.get(1)?;
    let status: String = row.get(2)?;
    let created_at: String = row.get(3)?;
    let updated_at: String = row.get(4)?;

    Ok(Note {
        id: Uuid::parse_str(&id).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?,
        raw_text,
        status: str_to_status(&status).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                2,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::other(e.to_string())),
            )
        })?,
        created_at: parse_dt(&created_at)?,
        updated_at: parse_dt(&updated_at)?,
    })
}

fn effort_to_str(effort: Effort) -> &'static str {
    match effort {
        Effort::Xs => "xs",
        Effort::S => "s",
        Effort::M => "m",
        Effort::L => "l",
    }
}

fn str_to_effort(s: &str) -> Result<Effort, StorageError> {
    match s {
        "xs" => Ok(Effort::Xs),
        "s" => Ok(Effort::S),
        "m" => Ok(Effort::M),
        "l" => Ok(Effort::L),
        other => Err(StorageError::InvalidData(format!("unknown effort: {other}"))),
    }
}

fn tag_kind_to_str(kind: TagKind) -> &'static str {
    match kind {
        TagKind::General => "general",
        TagKind::Custom => "custom",
    }
}

fn str_to_tag_kind(s: &str) -> Result<TagKind, StorageError> {
    match s {
        "general" => Ok(TagKind::General),
        "custom" => Ok(TagKind::Custom),
        other => Err(StorageError::InvalidData(format!("unknown tag kind: {other}"))),
    }
}

fn tag_status_to_str(status: TagStatus) -> &'static str {
    match status {
        TagStatus::Active => "active",
        TagStatus::PendingApproval => "pending_approval",
    }
}

fn str_to_tag_status(s: &str) -> Result<TagStatus, StorageError> {
    match s {
        "active" => Ok(TagStatus::Active),
        "pending_approval" => Ok(TagStatus::PendingApproval),
        other => Err(StorageError::InvalidData(format!("unknown tag status: {other}"))),
    }
}

fn decision_action_to_str(action: DecisionAction) -> &'static str {
    match action {
        DecisionAction::Accept => "accept",
        DecisionAction::Defer => "defer",
        DecisionAction::Dismiss => "dismiss",
        DecisionAction::Merge => "merge",
    }
}

fn row_to_action_item(row: &rusqlite::Row) -> rusqlite::Result<ActionItem> {
    let id: String = row.get(0)?;
    let note_id: String = row.get(1)?;
    let title: String = row.get(2)?;
    let effort: Option<String> = row.get(3)?;
    let due_date: Option<String> = row.get(4)?;
    let created_at: String = row.get(5)?;
    let classified_at: Option<String> = row.get(6)?;
    let active_at: Option<String> = row.get(7)?;
    let backlog_at: Option<String> = row.get(8)?;
    let done_at: Option<String> = row.get(9)?;
    let dismissed_at: Option<String> = row.get(10)?;

    Ok(ActionItem {
        id: parse_uuid(&id)?,
        note_id: parse_uuid(&note_id)?,
        title,
        effort: effort
            .map(|e| str_to_effort(&e))
            .transpose()
            .map_err(storage_err_to_sqlite)?,
        due_date: due_date.map(|d| parse_dt(&d)).transpose()?,
        created_at: parse_dt(&created_at)?,
        classified_at: classified_at.map(|d| parse_dt(&d)).transpose()?,
        active_at: active_at.map(|d| parse_dt(&d)).transpose()?,
        backlog_at: backlog_at.map(|d| parse_dt(&d)).transpose()?,
        done_at: done_at.map(|d| parse_dt(&d)).transpose()?,
        dismissed_at: dismissed_at.map(|d| parse_dt(&d)).transpose()?,
    })
}

fn row_to_tag(row: &rusqlite::Row) -> rusqlite::Result<Tag> {
    let id: String = row.get(0)?;
    let name: String = row.get(1)?;
    let kind: String = row.get(2)?;
    let status: String = row.get(3)?;
    let created_at: String = row.get(4)?;

    Ok(Tag {
        id: parse_uuid(&id)?,
        name,
        kind: str_to_tag_kind(&kind).map_err(storage_err_to_sqlite)?,
        status: str_to_tag_status(&status).map_err(storage_err_to_sqlite)?,
        created_at: parse_dt(&created_at)?,
    })
}

fn parse_uuid(s: &str) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn storage_err_to_sqlite(e: StorageError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::other(e.to_string())),
    )
}

fn parse_dt(s: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_get_note_roundtrips() {
        let store = Store::open_in_memory().unwrap();
        let note = Note {
            id: Uuid::new_v4(),
            raw_text: "call the vendor back".to_string(),
            status: NoteStatus::Editing,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        store.insert_note(&note).unwrap();
        let fetched = store.get_note(note.id).unwrap().unwrap();
        assert_eq!(fetched.raw_text, note.raw_text);
        assert_eq!(fetched.status, note.status);
    }

    #[test]
    fn get_note_returns_none_for_unknown_id() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.get_note(Uuid::new_v4()).unwrap().is_none());
    }

    #[test]
    fn reopening_the_same_database_does_not_reapply_migrations() {
        let path = std::env::temp_dir().join(format!("todoz-test-{}.sqlite3", Uuid::new_v4()));

        let note = Note {
            id: Uuid::new_v4(),
            raw_text: "persisted across reopen".to_string(),
            status: NoteStatus::Idle,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        {
            let store = Store::open(&path).unwrap();
            store.insert_note(&note).unwrap();
        }

        // Reopening must re-run migrate() without error (idempotent) and must
        // not have wiped prior data.
        let store = Store::open(&path).unwrap();
        let fetched = store.get_note(note.id).unwrap().unwrap();
        assert_eq!(fetched.raw_text, note.raw_text);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn update_note_persists_text_and_status() {
        let store = Store::open_in_memory().unwrap();
        let note = Note {
            id: Uuid::new_v4(),
            raw_text: "first draft".to_string(),
            status: NoteStatus::Editing,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        store.insert_note(&note).unwrap();

        store
            .update_note(note.id, "revised text", NoteStatus::Classified, Utc::now())
            .unwrap();

        let fetched = store.get_note(note.id).unwrap().unwrap();
        assert_eq!(fetched.raw_text, "revised text");
        assert_eq!(fetched.status, NoteStatus::Classified);
    }

    #[test]
    fn settings_default_to_none_then_roundtrip_after_set() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.get_setting("idle_timeout_s").unwrap().is_none());

        store.set_setting("idle_timeout_s", "60").unwrap();
        assert_eq!(
            store.get_setting("idle_timeout_s").unwrap().as_deref(),
            Some("60")
        );

        store.set_setting("idle_timeout_s", "90").unwrap();
        assert_eq!(
            store.get_setting("idle_timeout_s").unwrap().as_deref(),
            Some("90")
        );
    }

    fn insert_note_fixture(store: &Store, text: &str) -> Note {
        let note = Note {
            id: Uuid::new_v4(),
            raw_text: text.to_string(),
            status: NoteStatus::Classified,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        store.insert_note(&note).unwrap();
        note
    }

    fn insert_item_fixture(store: &Store, note_id: Uuid, title: &str) -> ActionItem {
        let item = ActionItem {
            id: Uuid::new_v4(),
            note_id,
            title: title.to_string(),
            effort: Some(Effort::S),
            due_date: None,
            created_at: Utc::now(),
            classified_at: Some(Utc::now()),
            active_at: None,
            backlog_at: None,
            done_at: None,
            dismissed_at: None,
        };
        store.insert_action_item(&item).unwrap();
        item
    }

    #[test]
    fn list_notes_orders_by_updated_at_desc() {
        let store = Store::open_in_memory().unwrap();
        let first = insert_note_fixture(&store, "first");
        std::thread::sleep(std::time::Duration::from_millis(5));
        let second = insert_note_fixture(&store, "second");

        let notes = store.list_notes().unwrap();
        assert_eq!(notes[0].id, second.id);
        assert_eq!(notes[1].id, first.id);
    }

    #[test]
    fn insert_and_list_action_items_for_note() {
        let store = Store::open_in_memory().unwrap();
        let note = insert_note_fixture(&store, "call jerry, need docs");
        insert_item_fixture(&store, note.id, "Call Jerry");
        insert_item_fixture(&store, note.id, "Get docs");

        let items = store.list_items_for_note(note.id).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "Call Jerry");
    }

    #[test]
    fn get_or_create_tag_reuses_existing_tag_by_name() {
        let store = Store::open_in_memory().unwrap();
        let first = store.get_or_create_tag("finance", TagKind::General).unwrap();
        let second = store.get_or_create_tag("finance", TagKind::General).unwrap();
        assert_eq!(first.id, second.id);
    }

    #[test]
    fn tag_card_counts_count_distinct_notes_not_items() {
        let store = Store::open_in_memory().unwrap();
        let note = insert_note_fixture(&store, "two items, one tag");
        let item_a = insert_item_fixture(&store, note.id, "Item A");
        let item_b = insert_item_fixture(&store, note.id, "Item B");
        let tag = store.get_or_create_tag("project-x", TagKind::Custom).unwrap();
        store.insert_item_tag(item_a.id, tag.id).unwrap();
        store.insert_item_tag(item_b.id, tag.id).unwrap();

        let counts = store.list_tags_with_card_counts().unwrap();
        let (_, count) = counts.into_iter().find(|(t, _)| t.id == tag.id).unwrap();
        assert_eq!(count, 1, "both items belong to the same note/card");
    }

    #[test]
    fn set_action_item_timestamp_updates_lifecycle_state() {
        let store = Store::open_in_memory().unwrap();
        let note = insert_note_fixture(&store, "finish report");
        let item = insert_item_fixture(&store, note.id, "Finish report");

        store
            .set_action_item_timestamp(item.id, "done_at", Utc::now())
            .unwrap();

        let fetched = store.get_action_item(item.id).unwrap().unwrap();
        assert!(fetched.done_at.is_some());
        assert_eq!(fetched.state(), todoz_core::ActionItemState::Done);
    }

    #[test]
    fn insert_decision_persists_without_error() {
        let store = Store::open_in_memory().unwrap();
        let note = insert_note_fixture(&store, "approve budget");
        let item = insert_item_fixture(&store, note.id, "Approve budget");

        let decision = Decision {
            id: Uuid::new_v4(),
            item_id: item.id,
            action: DecisionAction::Accept,
            at: Utc::now(),
        };
        store.insert_decision(&decision).unwrap();
    }

    #[test]
    fn delete_action_item_removes_it_and_its_tags() {
        let store = Store::open_in_memory().unwrap();
        let note = insert_note_fixture(&store, "stale draft item");
        let item = insert_item_fixture(&store, note.id, "Stale draft item");
        let tag = store.get_or_create_tag("misc", TagKind::General).unwrap();
        store.insert_item_tag(item.id, tag.id).unwrap();

        store.delete_action_item(item.id).unwrap();

        assert!(store.get_action_item(item.id).unwrap().is_none());
        assert!(store.list_tags_for_item(item.id).unwrap().is_empty());
    }
}
