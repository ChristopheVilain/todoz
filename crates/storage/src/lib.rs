use chrono::{DateTime, Utc};
use rusqlite::Connection;
use std::path::Path;
use todoz_core::{Note, NoteStatus};
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
}
