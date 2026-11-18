CREATE TABLE IF NOT EXISTS quick_note_folders (
 id TEXT PRIMARY KEY, kind TEXT NOT NULL CHECK(kind IN ('group','category')), name TEXT NOT NULL,
 aliases_json TEXT NOT NULL CHECK(json_valid(aliases_json)), sort_order INTEGER NOT NULL DEFAULT 0, archived INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS quick_notes (
 id TEXT PRIMARY KEY, title TEXT NOT NULL, note_type TEXT NOT NULL, group_id TEXT NOT NULL, category_id TEXT NOT NULL,
 lifecycle TEXT NOT NULL CHECK(lifecycle IN ('active','archived','trash')), pinned INTEGER NOT NULL,
 created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, revision INTEGER NOT NULL, payload_json TEXT NOT NULL CHECK(json_valid(payload_json))
);
CREATE INDEX IF NOT EXISTS quick_notes_list ON quick_notes(lifecycle,updated_at DESC);
CREATE TABLE IF NOT EXISTS quick_note_revisions (
 id TEXT PRIMARY KEY, note_id TEXT NOT NULL REFERENCES quick_notes(id) ON DELETE CASCADE, operation TEXT NOT NULL,
 actor TEXT NOT NULL, created_at INTEGER NOT NULL, before_json TEXT NOT NULL, after_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS quick_note_captures (
 id TEXT PRIMARY KEY, request_id TEXT NOT NULL UNIQUE, note_id TEXT REFERENCES quick_notes(id) ON DELETE CASCADE,
 source_text TEXT NOT NULL, source_json TEXT NOT NULL, captured_at INTEGER NOT NULL, model_json TEXT NOT NULL,
 status TEXT NOT NULL, error TEXT NOT NULL, result_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS quick_note_reminders (
 id TEXT PRIMARY KEY, note_id TEXT NOT NULL REFERENCES quick_notes(id) ON DELETE CASCADE, task_id TEXT NOT NULL,
 schedule_json TEXT NOT NULL CHECK(json_valid(schedule_json)), next_fire_at INTEGER NOT NULL, enabled INTEGER NOT NULL,
 revision INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS quick_note_reminder_due ON quick_note_reminders(enabled,next_fire_at);
CREATE TABLE IF NOT EXISTS quick_note_reminder_events (
 id TEXT PRIMARY KEY, reminder_id TEXT REFERENCES quick_note_reminders(id) ON DELETE SET NULL,
 note_id TEXT NOT NULL REFERENCES quick_notes(id) ON DELETE CASCADE, scheduled_at INTEGER NOT NULL,
 snoozed_until INTEGER, status TEXT NOT NULL, error TEXT NOT NULL DEFAULT '', read_at INTEGER,
 UNIQUE(reminder_id,scheduled_at)
);
