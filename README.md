# todoz

A personal, open-source desktop notes app (macOS/Linux) that behaves like a plain
notes app to write in, but automatically turns what you write into tracked,
actionable items. It is **not** a kanban board or issue tracker — the unit of
work is a simple action item, not a card you drag between columns.

Built with [Tauri 2](https://tauri.app/) (Rust core, vanilla HTML/CSS/JS frontend).

## Concepts

- **Note** — a raw block of text you typed. One Note = one Card in the UI.
- **Action Item** — a concrete, trackable task extracted from a Note by the
  classification provider. A Note can produce zero or more Action Items.
- **Lifecycle** — an Action Item's state (`inbox` / `active` / `backlog` /
  `done` / `dismissed`) is *derived*, not stored directly: it's whichever of
  `active_at` / `backlog_at` / `done_at` / `dismissed_at` has the most recent
  timestamp (no timestamps set = `inbox`, i.e. needs triage). This means
  "re-opening" a dismissed or done item is just stamping a fresh `active_at` —
  no separate state field or transition table needed.
- **Tags** — a flat list, either `general` (drawn from the classifier's small
  curated taxonomy) or `custom` (freeform, user- or classifier-suggested).
  Clicking a tag in the sidebar filters the Todo List; ⌘/Ctrl-click combines
  multiple tags with AND.
- **Classification provider** — pluggable (`ClassificationProvider` trait in
  `crates/providers`). Real adapters (Anthropic first, OpenAI/Mistral after)
  are stubs for now; a `MockProvider` stands in during development — it
  splits a note into lines, treating each top-level line as a new draft
  Action Item and each indented line as a sub-detail merged into the item
  above it (matching a tab-indentation note-taking convention).

## Project layout

```
crates/
  core/        domain model, lifecycle state derivation, scorekeeper
  providers/   ClassificationProvider trait + provider stubs + MockProvider
  storage/     SQLite persistence (rusqlite), migrations
src/           frontend (vanilla HTML/CSS/JS)
src-tauri/     Tauri app shell, command handlers, app state
```

Storage is local-only for now (SQLite in the app data dir), but the schema
(UUIDv4 primary keys, timestamps, soft deletes on notes) is designed to be
sync-ready without implementing actual multi-device sync yet.

## Action item controls

The set of controls shown on an Action Item row depends on its current
derived state:

| State       | Available actions                  |
|-------------|-------------------------------------|
| `inbox`     | Accept, Backlog, Dismiss            |
| `active`    | Done, Backlog, Dismiss              |
| `backlog`   | Activate (Accept), Done, Dismiss    |
| `done`      | Re-open, Dismiss                    |
| `dismissed` | Re-open                             |

`Delete` is always available regardless of state — it's a **permanent**
removal (distinct from Dismiss), meant for cleaning up a wrongly duplicated
item. If deleted by mistake, re-running classification on the parent note
regenerates it, since classification reconciles by title: an Inbox item is
always regenerated fresh, a triaged item is left alone and not duplicated,
and a triaged item whose title is missing (i.e. was deleted) is recreated.

Tags can also be added to any item manually via the "+ tag" field on its row
(autocompletes against existing tag names); manually-added tags are always
`custom`.

## Development

```sh
npm install
npm run tauri dev
```

Run the Rust test suite (core domain logic, storage, providers):

```sh
cargo test --workspace
```

## License

MIT — see [LICENSE](LICENSE).
