CREATE TABLE notes (
    id TEXT PRIMARY KEY,
    raw_text TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT
);

CREATE TABLE action_items (
    id TEXT PRIMARY KEY,
    note_id TEXT NOT NULL REFERENCES notes(id),
    title TEXT NOT NULL,
    effort TEXT,
    due_date TEXT,
    created_at TEXT NOT NULL,
    classified_at TEXT,
    active_at TEXT,
    backlog_at TEXT,
    done_at TEXT,
    dismissed_at TEXT
);

CREATE TABLE tags (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE note_tags (
    note_id TEXT NOT NULL REFERENCES notes(id),
    tag_id TEXT NOT NULL REFERENCES tags(id),
    applied_at TEXT NOT NULL,
    PRIMARY KEY (note_id, tag_id)
);

CREATE TABLE item_tags (
    item_id TEXT NOT NULL REFERENCES action_items(id),
    tag_id TEXT NOT NULL REFERENCES tags(id),
    applied_at TEXT NOT NULL,
    PRIMARY KEY (item_id, tag_id)
);

CREATE TABLE decisions (
    id TEXT PRIMARY KEY,
    item_id TEXT NOT NULL REFERENCES action_items(id),
    action TEXT NOT NULL,
    at TEXT NOT NULL
);

CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
