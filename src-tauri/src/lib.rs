use chrono::{DateTime, Utc};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{Manager, State};
use todoz_core::{ActionItem, ActionItemState, Decision, DecisionAction, Effort, Note, NoteStatus, TagKind};
use todoz_providers::mock::MockProvider;
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
        Self {
            id: note.id.to_string(),
            raw_text: note.raw_text,
            status: note_status_str(note.status).to_string(),
        }
    }
}

#[derive(Serialize)]
struct ActionItemDto {
    id: String,
    note_id: String,
    title: String,
    effort: Option<String>,
    due_date: Option<String>,
    state: String,
    tags: Vec<String>,
}

#[derive(Serialize)]
struct CardDto {
    note_id: String,
    raw_text: String,
    note_status: String,
    created_at: String,
    updated_at: String,
    needs_triage: bool,
    items: Vec<ActionItemDto>,
}

#[derive(Serialize)]
struct TagDto {
    id: String,
    name: String,
    kind: String,
    card_count: i64,
}

#[derive(Serialize)]
struct CardListResult {
    cards: Vec<CardDto>,
    effective_preset: String,
}

#[derive(Serialize)]
struct ClassifyOutcome {
    status: String,
    error: Option<String>,
    created_items: Vec<ActionItemDto>,
}

fn note_status_str(status: NoteStatus) -> &'static str {
    match status {
        NoteStatus::Editing => "editing",
        NoteStatus::Idle => "idle",
        NoteStatus::Classified => "classified",
    }
}

fn item_state_str(state: ActionItemState) -> &'static str {
    match state {
        ActionItemState::Inbox => "inbox",
        ActionItemState::Active => "active",
        ActionItemState::Backlog => "backlog",
        ActionItemState::Done => "done",
        ActionItemState::Dismissed => "dismissed",
    }
}

fn effort_str(effort: Effort) -> &'static str {
    match effort {
        Effort::Xs => "xs",
        Effort::S => "s",
        Effort::M => "m",
        Effort::L => "l",
    }
}

fn lock_store<'a>(state: &'a State<'a, AppState>) -> Result<std::sync::MutexGuard<'a, Store>, String> {
    state.store.lock().map_err(|_| "store lock poisoned".to_string())
}

fn item_to_dto(store: &Store, item: &ActionItem) -> Result<ActionItemDto, String> {
    let tags = store
        .list_tags_for_item(item.id)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|t| t.name)
        .collect();
    Ok(ActionItemDto {
        id: item.id.to_string(),
        note_id: item.note_id.to_string(),
        title: item.title.clone(),
        effort: item.effort.map(effort_str).map(str::to_string),
        due_date: item.due_date.map(|d| d.to_rfc3339()),
        state: item_state_str(item.state()).to_string(),
        tags,
    })
}

fn build_card(store: &Store, note: &Note) -> Result<CardDto, String> {
    let items = store
        .list_items_for_note(note.id)
        .map_err(|e| e.to_string())?;
    let needs_triage = items
        .iter()
        .any(|i| i.state() == ActionItemState::Inbox);
    let item_dtos = items
        .iter()
        .map(|i| item_to_dto(store, i))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CardDto {
        note_id: note.id.to_string(),
        raw_text: note.raw_text.clone(),
        note_status: note_status_str(note.status).to_string(),
        created_at: note.created_at.to_rfc3339(),
        updated_at: note.updated_at.to_rfc3339(),
        needs_triage,
        items: item_dtos,
    })
}

fn card_is_open(card: &CardDto) -> bool {
    card.items
        .iter()
        .any(|i| i.state == "active" || i.state == "backlog")
}

fn card_earliest_due(card: &CardDto) -> Option<DateTime<Utc>> {
    card.items
        .iter()
        .filter_map(|i| i.due_date.as_ref())
        .filter_map(|d| DateTime::parse_from_rfc3339(d).ok())
        .map(|d| d.with_timezone(&Utc))
        .min()
}

fn card_has_tag(card: &CardDto, tag_name: &str) -> bool {
    card.items.iter().any(|i| i.tags.iter().any(|t| t == tag_name))
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
/// it's still open. Uses the mock classifier for now (see
/// todoz_providers::mock) — real provider adapters are a separate, not-yet-
/// built piece of work.
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
            created_items: vec![],
        });
    }

    let provider = MockProvider;
    let request = ClassificationRequest {
        note_text: note.raw_text.clone(),
        existing_general_tags: vec![],
        existing_custom_tags: vec![],
    };

    let result = provider.classify(request).await;

    let store = lock_store(&state)?;
    match result {
        Ok(response) => {
            // Re-classifying (idle re-fire, "re-run classification", or
            // closing raw-note edit mode) must not pile up duplicate items,
            // but must still regenerate an item a user deleted by mistake —
            // so this reconciles by title rather than blanket-replacing:
            // still-untriaged (Inbox) items are always cleared and
            // regenerated fresh; a triaged item (accepted/backlog/done/
            // dismissed) is left alone AND skips re-creating a duplicate for
            // the same title, but if that title is gone entirely (the user
            // hard-deleted it), the next classify pass recreates it as a
            // fresh Inbox item.
            let existing = store.list_items_for_note(id).map_err(|e| e.to_string())?;
            let mut triaged_titles = std::collections::HashSet::new();
            for item in existing {
                if item.state() == ActionItemState::Inbox {
                    store.delete_action_item(item.id).map_err(|e| e.to_string())?;
                } else {
                    triaged_titles.insert(item.title);
                }
            }

            let mut created_items = Vec::new();
            for draft in response.action_items {
                if triaged_titles.contains(&draft.title) {
                    continue;
                }
                let item = ActionItem {
                    id: Uuid::new_v4(),
                    note_id: id,
                    title: draft.title,
                    effort: draft.effort,
                    due_date: draft.due_date,
                    created_at: Utc::now(),
                    classified_at: Some(Utc::now()),
                    active_at: None,
                    backlog_at: None,
                    done_at: None,
                    dismissed_at: None,
                };
                store.insert_action_item(&item).map_err(|e| e.to_string())?;

                for name in draft.general_tags {
                    let tag = store
                        .get_or_create_tag(&name, TagKind::General)
                        .map_err(|e| e.to_string())?;
                    store
                        .insert_item_tag(item.id, tag.id)
                        .map_err(|e| e.to_string())?;
                }
                for name in draft.custom_tags {
                    let tag = store
                        .get_or_create_tag(&name, TagKind::Custom)
                        .map_err(|e| e.to_string())?;
                    store
                        .insert_item_tag(item.id, tag.id)
                        .map_err(|e| e.to_string())?;
                }

                created_items.push(item_to_dto(&store, &item)?);
            }

            store
                .update_note(id, &note.raw_text, NoteStatus::Classified, Utc::now())
                .map_err(|e| e.to_string())?;
            Ok(ClassifyOutcome {
                status: "classified".to_string(),
                error: None,
                created_items,
            })
        }
        Err(err) => {
            store
                .update_note(id, &note.raw_text, NoteStatus::Idle, Utc::now())
                .map_err(|e| e.to_string())?;
            Ok(ClassifyOutcome {
                status: "idle".to_string(),
                error: Some(err.to_string()),
                created_items: vec![],
            })
        }
    }
}

#[tauri::command]
fn list_cards(
    state: State<AppState>,
    preset: Option<String>,
    tag_names: Vec<String>,
) -> Result<CardListResult, String> {
    let store = lock_store(&state)?;
    let notes = store.list_notes().map_err(|e| e.to_string())?;
    let mut cards = notes
        .iter()
        .map(|n| build_card(&store, n))
        .collect::<Result<Vec<_>, _>>()?;

    let effective_preset = preset.unwrap_or_else(|| {
        if cards.iter().any(|c| c.needs_triage) {
            "triage".to_string()
        } else {
            "open".to_string()
        }
    });

    cards.retain(|c| match effective_preset.as_str() {
        "triage" => c.needs_triage,
        "open" => card_is_open(c),
        _ => true, // "all"
    });

    if !tag_names.is_empty() {
        cards.retain(|c| tag_names.iter().all(|t| card_has_tag(c, t)));
    }

    match effective_preset.as_str() {
        "open" => cards.sort_by_key(card_earliest_due),
        _ => cards.sort_by(|a, b| b.updated_at.cmp(&a.updated_at)),
    }

    Ok(CardListResult {
        cards,
        effective_preset,
    })
}

#[tauri::command]
fn get_card(state: State<AppState>, note_id: String) -> Result<CardDto, String> {
    let id = Uuid::parse_str(&note_id).map_err(|e| e.to_string())?;
    let store = lock_store(&state)?;
    let note = store
        .get_note(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "note not found".to_string())?;
    build_card(&store, &note)
}

#[tauri::command]
fn list_tags(state: State<AppState>) -> Result<Vec<TagDto>, String> {
    let store = lock_store(&state)?;
    let tags = store.list_tags_with_card_counts().map_err(|e| e.to_string())?;
    Ok(tags
        .into_iter()
        .map(|(tag, count)| TagDto {
            id: tag.id.to_string(),
            name: tag.name,
            kind: match tag.kind {
                TagKind::General => "general".to_string(),
                TagKind::Custom => "custom".to_string(),
            },
            card_count: count,
        })
        .collect())
}

/// `action` is one of "accept" | "done" | "backlog" | "dismiss". Only accept/
/// backlog/dismiss are triage decisions and get logged to the Decision table
/// (per the data model, Decision is specifically the triage log — marking an
/// already-accepted item "done" later is normal completion, not a decision).
#[tauri::command]
fn act_on_item(state: State<AppState>, item_id: String, action: String) -> Result<ActionItemDto, String> {
    let id = Uuid::parse_str(&item_id).map_err(|e| e.to_string())?;
    let store = lock_store(&state)?;

    let (field, decision_action) = match action.as_str() {
        "accept" => ("active_at", Some(DecisionAction::Accept)),
        "done" => ("done_at", None),
        "backlog" => ("backlog_at", Some(DecisionAction::Defer)),
        "dismiss" => ("dismissed_at", Some(DecisionAction::Dismiss)),
        other => return Err(format!("unknown action: {other}")),
    };

    let now = Utc::now();
    store
        .set_action_item_timestamp(id, field, now)
        .map_err(|e| e.to_string())?;

    if let Some(decision_action) = decision_action {
        let decision = Decision {
            id: Uuid::new_v4(),
            item_id: id,
            action: decision_action,
            at: now,
        };
        store.insert_decision(&decision).map_err(|e| e.to_string())?;
    }

    let item = store
        .get_action_item(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "action item not found".to_string())?;
    item_to_dto(&store, &item)
}

/// Permanent removal — distinct from "dismiss" (which keeps the item, just
/// marks it resolved-as-not-doing). Meant for cleaning up a wrongly
/// duplicated item; if that was a mistake, re-running classification
/// regenerates it from the raw note (see classify_note's reconciliation).
#[tauri::command]
fn delete_item(state: State<AppState>, item_id: String) -> Result<(), String> {
    let id = Uuid::parse_str(&item_id).map_err(|e| e.to_string())?;
    lock_store(&state)?.delete_action_item(id).map_err(|e| e.to_string())
}

#[tauri::command]
fn add_tag_to_item(state: State<AppState>, item_id: String, tag_name: String) -> Result<ActionItemDto, String> {
    let id = Uuid::parse_str(&item_id).map_err(|e| e.to_string())?;
    let store = lock_store(&state)?;
    let name = tag_name.trim();
    if name.is_empty() {
        return Err("tag name cannot be empty".to_string());
    }
    // Manually-typed tags are treated as custom (user-defined, per-subject),
    // as opposed to the small curated general taxonomy the classifier draws
    // from — get_or_create_tag reuses an existing tag of either kind by name.
    let tag = store.get_or_create_tag(name, TagKind::Custom).map_err(|e| e.to_string())?;
    store.insert_item_tag(id, tag.id).map_err(|e| e.to_string())?;
    let item = store
        .get_action_item(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "action item not found".to_string())?;
    item_to_dto(&store, &item)
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
            list_cards,
            get_card,
            list_tags,
            act_on_item,
            delete_item,
            add_tag_to_item,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
