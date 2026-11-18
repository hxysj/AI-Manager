-- 日历规则独立于便签，保留修改与删除历史，恢复备份时不丢失个人排班。
CREATE TABLE IF NOT EXISTS quick_note_calendar_rules (
  id TEXT PRIMARY KEY NOT NULL,
  revision INTEGER NOT NULL,
  payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  deleted_at INTEGER
);
CREATE TABLE IF NOT EXISTS quick_note_calendar_rule_revisions (
  id TEXT PRIMARY KEY NOT NULL,
  rule_id TEXT NOT NULL REFERENCES quick_note_calendar_rules(id) ON DELETE CASCADE,
  operation TEXT NOT NULL,
  actor TEXT NOT NULL,
  before_json TEXT NOT NULL,
  after_json TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS quick_note_calendar_rule_history ON quick_note_calendar_rule_revisions(rule_id,created_at);
