use chrono::Utc;
use serde::Serialize;
use std::sync::Mutex;
use tauri::{Manager, State};
use todoz_core::{Note, NoteStatus};
use todoz_providers::anthropic::AnthropicProvider;
use todoz_providers::{ClassificationProvider, ClassificationRequest};
use todoz_storage::Store;
use uuid::Uuid;

const DEFAULT_IDLE_TIMEOUT_S: u32 = 60;

struct AppState {
    store: Mutex<Store>,
}

#[derive(Serialize)]
struct NoteDto {
    id: String,
    raw_text: String,
    status: String,
}

impl From<Note> for NoteDto {
    fn from(note: Note) -> Self {
        let status = match note.status {
            NoteStatus::Editing => "editing",
            NoteStatus::Idle => "idle",
            NoteStatus::Classified => "classified",
        };
        Self {
            id: note.id.to_string(),
            raw_text: note.raw_text,
            status: status.to_string(),
        }
    }
}

#[derive(Serialize)]
struct ClassifyOutcome {
    status: String,
    error: Option<String>,
}

fn lock_store<'a>(state: &'a State<'a, AppState>) -> Result<std::sync::MutexGuard<'a, Store>, String> {
    state.store.lock().map_err(|_| "store lock poisoned".to_string())
}

#[tauri::command]
fn create_note(state: State<AppState>) -> Result<NoteDto, String> {
    let note = Note {
        id: Uuid::new_v4(),
        raw_text: String::new(),
        status: NoteStatus::Editing,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    lock_store(&state)?
        .insert_note(&note)
        .map_err(|e| e.to_string())?;
    Ok(note.into())
}

#[tauri::command]
fn update_note_text(state: State<AppState>, note_id: String, raw_text: String) -> Result<(), String> {
    let id = Uuid::parse_str(&note_id).map_err(|e| e.to_string())?;
    lock_store(&state)?
        .update_note(id, &raw_text, NoteStatus::Editing, Utc::now())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_idle_timeout(state: State<AppState>) -> Result<u32, String> {
    let value = lock_store(&state)?
        .get_setting("idle_timeout_s")
        .map_err(|e| e.to_string())?;
    Ok(value
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_IDLE_TIMEOUT_S))
}

#[tauri::command]
fn set_idle_timeout(state: State<AppState>, seconds: u32) -> Result<(), String> {
    lock_store(&state)?
        .set_setting("idle_timeout_s", &seconds.to_string())
        .map_err(|e| e.to_string())
}

/// Fires when the capture panel closes, or when the idle timer elapses while
/// it's still open — both paths funnel through this single command.
#[tauri::command]
async fn classify_note(state: State<'_, AppState>, note_id: String) -> Result<ClassifyOutcome, String> {
    let id = Uuid::parse_str(&note_id).map_err(|e| e.to_string())?;

    let note = {
        let store = lock_store(&state)?;
        store
            .get_note(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "note not found".to_string())?
    };

    if note.raw_text.trim().is_empty() {
        return Ok(ClassifyOutcome {
            status: "idle".to_string(),
            error: None,
        });
    }

    // TODO: pick provider/model from Settings.active_provider once more than
    // one adapter is implemented; API key from OS keychain instead of env var.
    let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
    let provider = AnthropicProvider::new(api_key, "claude-sonnet-5".to_string());
    let request = ClassificationRequest {
        note_text: note.raw_text.clone(),
        existing_general_tags: vec![],
        existing_custom_tags: vec![],
    };

    let result = provider.classify(request).await;

    let store = lock_store(&state)?;
    match result {
        Ok(_response) => {
            store
                .update_note(id, &note.raw_text, NoteStatus::Classified, Utc::now())
                .map_err(|e| e.to_string())?;
            Ok(ClassifyOutcome {
                status: "classified".to_string(),
                error: None,
            })
        }
        Err(err) => {
            store
                .update_note(id, &note.raw_text, NoteStatus::Idle, Utc::now())
                .map_err(|e| e.to_string())?;
            Ok(ClassifyOutcome {
                status: "idle".to_string(),
                error: Some(err.to_string()),
            })
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir().expect("resolve app data dir");
            std::fs::create_dir_all(&app_data_dir).expect("create app data dir");
            let store = Store::open(app_data_dir.join("todoz.sqlite3")).expect("open sqlite store");
            app.manage(AppState {
                store: Mutex::new(store),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            create_note,
            update_note_text,
            classify_note,
            get_idle_timeout,
            set_idle_timeout,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
