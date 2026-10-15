const { invoke } = window.__TAURI__.core;

const AUTOSAVE_DEBOUNCE_MS = 800;

// --- DOM refs (populated on DOMContentLoaded) ---
let cardDetailEl, cardListEl, tagsListEl;
let filterPresetsEl, activeFilterIndicatorEl, activeFilterLabelEl, clearFilterButtonEl;
let newNoteButtonEl, logoSettingsButtonEl;
let settingsOverlayEl, idleTimeoutInputEl, settingsCloseButtonEl;
let captureOverlayEl, captureTextareaEl, llmLogListEl, suggestedTagsListEl, captureDoneButtonEl;
let errorBannerEl, tagOptionsEl;

// --- App state ---
let selectedNoteId = null;
let currentCard = null; // CardDto from get_card, or null
let rawNoteEditMode = false;
let pinnedPreset = null; // "triage" | "open" | "all" | null (auto)
let selectedTags = []; // tag names, ANDed

// --- Capture overlay state ---
let currentNote = null; // { id, rawText }
let idleTimeoutSeconds = 60;
let autosaveTimer = null;
let idleTimer = null;

function isMac() {
  return navigator.platform.toUpperCase().includes("MAC");
}

// Textareas here are used for note-taking where Tab indicates a sub-item
// relationship to the line above — it must insert a literal tab character,
// never shift focus to the next element (the browser default, which made it
// look like the panel couldn't be closed).
function enableTabIndent(textarea) {
  textarea.addEventListener("keydown", (event) => {
    if (event.key !== "Tab") return;
    event.preventDefault();
    const { selectionStart, selectionEnd, value } = textarea;
    textarea.value = value.slice(0, selectionStart) + "\t" + value.slice(selectionEnd);
    textarea.selectionStart = textarea.selectionEnd = selectionStart + 1;
    textarea.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

function presetLabel(preset) {
  return { triage: "Needs Triage", open: "Open", all: "All" }[preset] ?? preset;
}

// Surfaces otherwise-silent invoke() failures (e.g. the delete/tag/act
// commands previously failed without any visible feedback, which read as
// "the button does nothing") as a transient banner plus a console entry.
function showError(err) {
  console.error(err);
  if (!errorBannerEl) return;
  const message = typeof err === "string" ? err : err?.message || String(err);
  errorBannerEl.textContent = message;
  errorBannerEl.hidden = false;
  clearTimeout(showError._timer);
  showError._timer = setTimeout(() => {
    errorBannerEl.hidden = true;
  }, 6000);
}

// ---------------------------------------------------------------------------
// Tags sidebar
// ---------------------------------------------------------------------------

async function loadTags() {
  const tags = await invoke("list_tags");
  tagsListEl.innerHTML = "";
  for (const tag of tags) {
    const li = document.createElement("li");
    li.className = "tag-item";
    li.textContent = `${tag.name} — ${tag.card_count}`;
    li.dataset.tagName = tag.name;
    if (selectedTags.includes(tag.name)) li.classList.add("selected");
    li.addEventListener("click", (event) => onTagClick(tag.name, event));
    tagsListEl.appendChild(li);
  }

  if (tagOptionsEl) {
    tagOptionsEl.innerHTML = tags.map((t) => `<option value="${escapeHtml(t.name)}"></option>`).join("");
  }
}

function onTagClick(tagName, event) {
  const additive = isMac() ? event.metaKey : event.ctrlKey;
  if (additive) {
    const idx = selectedTags.indexOf(tagName);
    if (idx === -1) selectedTags.push(tagName);
    else selectedTags.splice(idx, 1);
  } else if (selectedTags.length === 1 && selectedTags[0] === tagName) {
    selectedTags = [];
  } else {
    selectedTags = [tagName];
  }
  renderTagSelection();
  loadCardList();
}

function renderTagSelection() {
  for (const li of tagsListEl.querySelectorAll(".tag-item")) {
    li.classList.toggle("selected", selectedTags.includes(li.dataset.tagName));
  }
}

// ---------------------------------------------------------------------------
// Todo list (right sidebar)
// ---------------------------------------------------------------------------

const STATE_ORDER = ["inbox", "active", "backlog", "done", "dismissed"];
const STATE_SHORT_LABEL = {
  inbox: "inbox",
  active: "active",
  backlog: "backlog",
  done: "done",
  dismissed: "dismissed",
};

// Per-card breakdown shown in the Todo List row, e.g. "1 inbox · 2 active" —
// previously the row only showed a bare item count with no lifecycle state.
function cardStatusSummary(card) {
  if (card.items.length === 0) return "no items yet";
  const counts = {};
  for (const item of card.items) counts[item.state] = (counts[item.state] || 0) + 1;
  return STATE_ORDER.filter((state) => counts[state])
    .map((state) => `${counts[state]} ${STATE_SHORT_LABEL[state]}`)
    .join(" · ");
}

async function loadCardList() {
  const result = await invoke("list_cards", {
    preset: pinnedPreset,
    tagNames: selectedTags,
  });

  for (const button of filterPresetsEl.querySelectorAll("button")) {
    button.classList.toggle("active", button.dataset.preset === result.effective_preset);
  }

  const filterActive = pinnedPreset !== null || selectedTags.length > 0;
  activeFilterIndicatorEl.hidden = !filterActive;
  if (filterActive) {
    const parts = [];
    if (pinnedPreset) parts.push(presetLabel(pinnedPreset));
    if (selectedTags.length) parts.push(`tags: ${selectedTags.join(", ")}`);
    activeFilterLabelEl.textContent = parts.join(" + ");
  }

  cardListEl.innerHTML = "";
  for (const card of result.cards) {
    const li = document.createElement("li");
    li.className = "card-list-item";
    if (card.note_id === selectedNoteId) li.classList.add("selected");
    if (card.needs_triage) li.classList.add("needs-triage");

    const snippet = card.raw_text.split("\n").find((l) => l.trim()) || "(empty note)";
    li.innerHTML = `<span class="card-snippet"></span><span class="card-meta">${escapeHtml(cardStatusSummary(card))}</span>`;
    li.querySelector(".card-snippet").textContent = snippet;

    li.addEventListener("click", () => selectCard(card.note_id));
    cardListEl.appendChild(li);
  }
}

function clearFilter() {
  pinnedPreset = null;
  selectedTags = [];
  renderTagSelection();
  loadCardList();
}

// ---------------------------------------------------------------------------
// Card detail (center panel)
// ---------------------------------------------------------------------------

async function selectCard(noteId) {
  selectedNoteId = noteId;
  rawNoteEditMode = false;
  currentCard = await invoke("get_card", { noteId });
  renderCardDetail();
  loadCardList(); // refresh selection highlight
}

function renderCardDetail() {
  cardDetailEl.classList.toggle("has-selection", !!currentCard);

  if (!currentCard) {
    cardDetailEl.innerHTML = `<div class="empty-state"><p>Select a card, or press <kbd>⌘N</kbd> / <kbd>Ctrl+N</kbd> to capture something new.</p></div>`;
    return;
  }

  if (rawNoteEditMode) {
    renderRawNoteMode();
    return;
  }

  renderCardLevelView();
}

function cardTitle(card) {
  return card.raw_text.split("\n").find((l) => l.trim()) || "(empty note)";
}

// Which lifecycle actions make sense from a given state — notably, a
// dismissed or done item previously still showed a "Dismiss" button (dead
// end, no way back) instead of a way to undo it. "Re-open" reuses the
// "accept" action: act_on_item just stamps active_at to now, and state is
// derived from whichever timestamp is most recent, so this naturally wins
// over the older dismissed_at/done_at without any backend change.
function controlsForItem(item) {
  switch (item.state) {
    case "inbox":
      return [
        { action: "accept", label: "✓ Accept" },
        { action: "backlog", label: "⏷ Backlog" },
        { action: "dismiss", label: "✕ Dismiss" },
      ];
    case "active":
      return [
        { action: "done", label: "✔ Done" },
        { action: "backlog", label: "⏷ Backlog" },
        { action: "dismiss", label: "✕ Dismiss" },
      ];
    case "backlog":
      return [
        { action: "accept", label: "✓ Activate" },
        { action: "done", label: "✔ Done" },
        { action: "dismiss", label: "✕ Dismiss" },
      ];
    case "done":
      return [
        { action: "accept", label: "↺ Re-open" },
        { action: "dismiss", label: "✕ Dismiss" },
      ];
    case "dismissed":
      return [{ action: "accept", label: "↺ Re-open" }];
    default:
      return [];
  }
}

function renderCardLevelView() {
  const card = currentCard;
  cardDetailEl.innerHTML = "";

  const title = document.createElement("h2");
  title.className = "card-title";
  title.textContent = cardTitle(card);
  cardDetailEl.appendChild(title);

  const toolbar = document.createElement("div");
  toolbar.className = "view-panel-controls";
  toolbar.innerHTML = `
    <button id="dismiss-card-button">Dismiss card</button>
    <button id="rerun-classification-button">Re-run classification</button>
    <button id="switch-to-raw-note-button">Raw note</button>
  `;
  cardDetailEl.appendChild(toolbar);

  const checklist = document.createElement("div");
  checklist.className = "action-item-checklist";
  if (card.items.length === 0) {
    checklist.innerHTML = `<p class="placeholder-note">No action items extracted from this note.</p>`;
  }
  for (const item of card.items) {
    const row = document.createElement("div");
    row.className = `action-item-row state-${item.state}`;
    const controlsHtml = controlsForItem(item)
      .map((c) => `<button type="button" data-action="${c.action}">${c.label}</button>`)
      .join("");
    const tagsHtml = item.tags.length
      ? item.tags.map((t) => `<span class="tag-chip">${escapeHtml(t)}</span>`).join("")
      : `<span class="placeholder-note">no tags</span>`;
    row.innerHTML = `
      <span class="action-item-title">${escapeHtml(item.title)}</span>
      <span class="action-item-state">${item.state}</span>
      ${item.state === "inbox" ? '<span class="placeholder-note">New — Accept, Backlog, or Dismiss</span>' : ""}
      <span class="action-item-tags">${tagsHtml}</span>
      <span class="action-item-add-tag">
        <input type="text" class="add-tag-input" placeholder="add tag…" size="8" list="tag-options" />
        <button type="button" class="add-tag-button" title="Add tag">+</button>
      </span>
      <span class="action-item-controls">
        ${controlsHtml}
        <button type="button" class="delete-item-button" title="Permanently delete (re-classify to recover if unintentional)">🗑 Delete</button>
      </span>
    `;
    for (const button of row.querySelectorAll("button[data-action]")) {
      button.addEventListener("click", () => actOnItem(item.id, button.dataset.action));
    }
    row.querySelector(".delete-item-button").addEventListener("click", () => deleteItem(item.id));
    const addTagInput = row.querySelector(".add-tag-input");
    const submitTag = () => {
      const name = addTagInput.value.trim();
      if (!name) return;
      addTagInput.value = "";
      addTagToItem(item.id, name);
    };
    addTagInput.addEventListener("keydown", (event) => {
      if (event.key !== "Enter") return;
      submitTag();
    });
    row.querySelector(".add-tag-button").addEventListener("click", submitTag);
    checklist.appendChild(row);
  }
  cardDetailEl.appendChild(checklist);

  const rawNoteViewport = document.createElement("section");
  rawNoteViewport.className = "raw-note-viewport";
  rawNoteViewport.innerHTML = `
    <h3>Raw Note</h3>
    <pre class="raw-note-viewport-text"></pre>
  `;
  rawNoteViewport.querySelector(".raw-note-viewport-text").textContent =
    card.raw_text || "(empty note)";
  cardDetailEl.appendChild(rawNoteViewport);

  document.querySelector("#dismiss-card-button").addEventListener("click", dismissCard);
  document.querySelector("#rerun-classification-button").addEventListener("click", reRunClassification);
  document.querySelector("#switch-to-raw-note-button").addEventListener("click", () => {
    rawNoteEditMode = true;
    renderCardDetail();
  });
}

function renderRawNoteMode() {
  const card = currentCard;
  cardDetailEl.innerHTML = `
    <h2 class="card-title">${escapeHtml(cardTitle(card))}</h2>
    <div class="view-panel-controls">
      <button id="close-raw-note-button">Close, back to card view</button>
    </div>
    <textarea id="raw-note-textarea" class="raw-note-textarea"></textarea>
    <p id="raw-note-status" class="status"></p>
  `;
  const rawNoteTextareaEl = document.querySelector("#raw-note-textarea");
  rawNoteTextareaEl.value = card.raw_text;
  enableTabIndent(rawNoteTextareaEl);
  document.querySelector("#close-raw-note-button").addEventListener("click", closeRawNoteMode);
}

async function closeRawNoteMode() {
  const text = document.querySelector("#raw-note-textarea").value;
  const statusEl = document.querySelector("#raw-note-status");
  await invoke("update_note_text", { noteId: selectedNoteId, rawText: text });
  if (statusEl) statusEl.textContent = "Classifying…";
  await invoke("classify_note", { noteId: selectedNoteId });
  rawNoteEditMode = false;
  currentCard = await invoke("get_card", { noteId: selectedNoteId });
  renderCardDetail();
  loadCardList();
  loadTags();
}

async function actOnItem(itemId, action) {
  try {
    await invoke("act_on_item", { itemId, action });
    currentCard = await invoke("get_card", { noteId: selectedNoteId });
    renderCardDetail();
    loadCardList();
  } catch (err) {
    showError(err);
  }
}

async function deleteItem(itemId) {
  try {
    await invoke("delete_item", { itemId });
    currentCard = await invoke("get_card", { noteId: selectedNoteId });
    renderCardDetail();
    loadCardList();
  } catch (err) {
    showError(err);
  }
}

async function addTagToItem(itemId, tagName) {
  try {
    await invoke("add_tag_to_item", { itemId, tagName });
    currentCard = await invoke("get_card", { noteId: selectedNoteId });
    renderCardDetail();
    loadTags();
  } catch (err) {
    showError(err);
  }
}

async function dismissCard() {
  for (const item of currentCard.items) {
    await invoke("act_on_item", { itemId: item.id, action: "dismiss" });
  }
  currentCard = await invoke("get_card", { noteId: selectedNoteId });
  renderCardDetail();
  loadCardList();
}

async function reRunClassification() {
  await invoke("classify_note", { noteId: selectedNoteId });
  currentCard = await invoke("get_card", { noteId: selectedNoteId });
  renderCardDetail();
  loadCardList();
  loadTags();
}

function escapeHtml(text) {
  const div = document.createElement("div");
  div.textContent = text;
  return div.innerHTML;
}

// ---------------------------------------------------------------------------
// Settings overlay
// ---------------------------------------------------------------------------

async function openSettings() {
  idleTimeoutInputEl.value = await invoke("get_idle_timeout");
  settingsOverlayEl.hidden = false;
}

function closeSettings() {
  settingsOverlayEl.hidden = true;
}

async function onIdleTimeoutChange() {
  const seconds = Math.max(5, parseInt(idleTimeoutInputEl.value, 10) || 60);
  idleTimeoutSeconds = seconds;
  idleTimeoutInputEl.value = seconds;
  await invoke("set_idle_timeout", { seconds });
}

// ---------------------------------------------------------------------------
// Capture overlay (new note)
// ---------------------------------------------------------------------------

function resetIdleTimer() {
  if (idleTimer) clearTimeout(idleTimer);
  idleTimer = setTimeout(() => {
    classifyAndShow({ keepOpen: true });
  }, idleTimeoutSeconds * 1000);
}

function scheduleAutosave() {
  if (autosaveTimer) clearTimeout(autosaveTimer);
  autosaveTimer = setTimeout(flushAutosave, AUTOSAVE_DEBOUNCE_MS);
}

async function flushAutosave() {
  if (autosaveTimer) {
    clearTimeout(autosaveTimer);
    autosaveTimer = null;
  }
  if (!currentNote) return;
  currentNote.rawText = captureTextareaEl.value;
  await invoke("update_note_text", {
    noteId: currentNote.id,
    rawText: currentNote.rawText,
  });
}

async function classifyAndShow({ keepOpen }) {
  if (!currentNote) return;
  await flushAutosave();
  llmLogListEl.innerHTML = `<li class="log-line">Classifying…</li>`;
  const outcome = await invoke("classify_note", { noteId: currentNote.id });
  if (!currentNote) return; // closed while awaiting

  llmLogListEl.innerHTML = "";
  if (outcome.error) {
    const li = document.createElement("li");
    li.className = "log-line log-error";
    li.textContent = `Classification unavailable: ${outcome.error}`;
    llmLogListEl.appendChild(li);
  } else if (outcome.created_items.length === 0) {
    llmLogListEl.innerHTML = `<li class="log-line">No action items found.</li>`;
  } else {
    for (const item of outcome.created_items) {
      const li = document.createElement("li");
      li.className = "log-line";
      li.textContent = `Fetch Action Item → ${item.title}`;
      llmLogListEl.appendChild(li);
    }
  }

  const tagNames = [...new Set(outcome.created_items.flatMap((i) => i.tags))];
  suggestedTagsListEl.innerHTML = tagNames.length
    ? tagNames.map((t) => `<span class="tag-chip">${escapeHtml(t)}</span>`).join("")
    : `<span class="placeholder-note">None yet</span>`;

  if (keepOpen) resetIdleTimer();
}

async function openNewNote() {
  if (currentNote) return; // one note at a time for now
  const note = await invoke("create_note");
  currentNote = { id: note.id, rawText: "" };

  captureTextareaEl.value = "";
  llmLogListEl.innerHTML = "";
  suggestedTagsListEl.innerHTML = "";
  captureOverlayEl.hidden = false;
  captureTextareaEl.focus();
  resetIdleTimer();
}

async function closeCurrentNote() {
  if (!currentNote) return;
  if (idleTimer) {
    clearTimeout(idleTimer);
    idleTimer = null;
  }
  await classifyAndShow({ keepOpen: false });
  currentNote = null;
  captureOverlayEl.hidden = true;
  loadCardList();
  loadTags();
}

// ---------------------------------------------------------------------------
// Wiring
// ---------------------------------------------------------------------------

window.addEventListener("DOMContentLoaded", () => {
  cardDetailEl = document.querySelector("#card-detail");
  cardListEl = document.querySelector("#card-list");
  tagsListEl = document.querySelector("#tags-list");
  filterPresetsEl = document.querySelector("#filter-presets");
  activeFilterIndicatorEl = document.querySelector("#active-filter-indicator");
  activeFilterLabelEl = document.querySelector("#active-filter-label");
  clearFilterButtonEl = document.querySelector("#clear-filter-button");
  newNoteButtonEl = document.querySelector("#new-note-button");
  logoSettingsButtonEl = document.querySelector("#logo-settings-button");

  settingsOverlayEl = document.querySelector("#settings-overlay");
  idleTimeoutInputEl = document.querySelector("#idle-timeout-input");
  settingsCloseButtonEl = document.querySelector("#settings-close-button");

  captureOverlayEl = document.querySelector("#capture-overlay");
  captureTextareaEl = document.querySelector("#capture-textarea");
  llmLogListEl = document.querySelector("#llm-log-list");
  suggestedTagsListEl = document.querySelector("#suggested-tags-list");
  captureDoneButtonEl = document.querySelector("#capture-done-button");

  errorBannerEl = document.querySelector("#error-banner");
  tagOptionsEl = document.querySelector("#tag-options");

  for (const button of filterPresetsEl.querySelectorAll("button")) {
    button.addEventListener("click", () => {
      pinnedPreset = button.dataset.preset;
      loadCardList();
    });
  }
  clearFilterButtonEl.addEventListener("click", clearFilter);

  newNoteButtonEl.addEventListener("click", openNewNote);
  captureDoneButtonEl.addEventListener("click", closeCurrentNote);
  captureTextareaEl.addEventListener("input", () => {
    scheduleAutosave();
    resetIdleTimer();
  });
  enableTabIndent(captureTextareaEl);

  logoSettingsButtonEl.addEventListener("click", openSettings);
  settingsCloseButtonEl.addEventListener("click", closeSettings);
  idleTimeoutInputEl.addEventListener("change", onIdleTimeoutChange);

  window.addEventListener("keydown", (event) => {
    const modifierHeld = isMac() ? event.metaKey : event.ctrlKey;
    if (modifierHeld && event.key.toLowerCase() === "n") {
      event.preventDefault();
      openNewNote();
    } else if (event.key === "Escape") {
      if (currentNote) {
        event.preventDefault();
        closeCurrentNote();
      } else if (!settingsOverlayEl.hidden) {
        closeSettings();
      } else if (rawNoteEditMode) {
        event.preventDefault();
        closeRawNoteMode();
      }
    }
  });

  invoke("get_idle_timeout").then((seconds) => {
    idleTimeoutSeconds = seconds;
  });
  loadTags();
  loadCardList();
});
