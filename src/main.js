const { invoke } = window.__TAURI__.core;

const AUTOSAVE_DEBOUNCE_MS = 800;

let emptyStateEl;
let capturePanelEl;
let textareaEl;
let statusEl;
let newNoteButtonEl;
let doneButtonEl;
let idleTimeoutInputEl;

let currentNote = null; // { id, rawText }
let idleTimeoutSeconds = 60;
let autosaveTimer = null;
let idleTimer = null;

function isMac() {
  return navigator.platform.toUpperCase().includes("MAC");
}

function setStatus(text) {
  statusEl.textContent = text;
}

function resetIdleTimer() {
  if (idleTimer) clearTimeout(idleTimer);
  idleTimer = setTimeout(() => {
    classifyCurrentNote({ keepOpen: true });
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
  currentNote.rawText = textareaEl.value;
  await invoke("update_note_text", {
    noteId: currentNote.id,
    rawText: currentNote.rawText,
  });
}

async function classifyCurrentNote({ keepOpen }) {
  if (!currentNote) return;
  await flushAutosave();
  setStatus("Classifying…");
  const outcome = await invoke("classify_note", { noteId: currentNote.id });
  if (!currentNote) return; // panel was closed while awaiting

  if (outcome.status === "classified") {
    setStatus("Classified");
  } else if (outcome.error) {
    setStatus(`Saved (classification unavailable: ${outcome.error})`);
  } else {
    setStatus("Saved");
  }

  if (keepOpen) {
    resetIdleTimer();
  }
}

async function openNewNote() {
  if (currentNote) return; // one note at a time for now
  const note = await invoke("create_note");
  currentNote = { id: note.id, rawText: "" };

  textareaEl.value = "";
  setStatus("Draft");
  emptyStateEl.hidden = true;
  capturePanelEl.hidden = false;
  textareaEl.focus();
  resetIdleTimer();
}

async function closeCurrentNote() {
  if (!currentNote) return;
  if (idleTimer) {
    clearTimeout(idleTimer);
    idleTimer = null;
  }
  await classifyCurrentNote({ keepOpen: false });
  currentNote = null;
  capturePanelEl.hidden = true;
  emptyStateEl.hidden = false;
}

async function loadIdleTimeoutSetting() {
  idleTimeoutSeconds = await invoke("get_idle_timeout");
  idleTimeoutInputEl.value = idleTimeoutSeconds;
}

async function onIdleTimeoutChange() {
  const seconds = Math.max(5, parseInt(idleTimeoutInputEl.value, 10) || 60);
  idleTimeoutSeconds = seconds;
  idleTimeoutInputEl.value = seconds;
  await invoke("set_idle_timeout", { seconds });
}

window.addEventListener("DOMContentLoaded", () => {
  emptyStateEl = document.querySelector("#empty-state");
  capturePanelEl = document.querySelector("#capture-panel");
  textareaEl = document.querySelector("#capture-textarea");
  statusEl = document.querySelector("#capture-status");
  newNoteButtonEl = document.querySelector("#new-note-button");
  doneButtonEl = document.querySelector("#done-button");
  idleTimeoutInputEl = document.querySelector("#idle-timeout-input");

  newNoteButtonEl.addEventListener("click", openNewNote);
  doneButtonEl.addEventListener("click", closeCurrentNote);
  idleTimeoutInputEl.addEventListener("change", onIdleTimeoutChange);

  textareaEl.addEventListener("input", () => {
    scheduleAutosave();
    resetIdleTimer();
  });

  window.addEventListener("keydown", (event) => {
    const modifierHeld = isMac() ? event.metaKey : event.ctrlKey;
    if (modifierHeld && event.key.toLowerCase() === "n") {
      event.preventDefault();
      openNewNote();
    } else if (event.key === "Escape" && currentNote) {
      event.preventDefault();
      closeCurrentNote();
    }
  });

  loadIdleTimeoutSetting();
});
