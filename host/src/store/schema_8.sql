BEGIN;
ALTER TABLE workflows ADD COLUMN source TEXT NOT NULL DEFAULT 'graph';
ALTER TABLE workflows ADD COLUMN script_text TEXT NOT NULL DEFAULT '';
ALTER TABLE workflow_versions ADD COLUMN script_text TEXT NOT NULL DEFAULT '';
ALTER TABLE workflow_runs ADD COLUMN exec TEXT NOT NULL DEFAULT 'graph';
ALTER TABLE workflow_runs ADD COLUMN script_text TEXT NOT NULL DEFAULT '';
ALTER TABLE workflow_runs ADD COLUMN output TEXT;
ALTER TABLE workflow_steps ADD COLUMN label TEXT;
ALTER TABLE workflow_steps ADD COLUMN phase TEXT;
ALTER TABLE workflow_steps ADD COLUMN schema_json TEXT;
CREATE TABLE IF NOT EXISTS workflow_run_notes (
  run_id TEXT NOT NULL,
  position INTEGER NOT NULL CHECK(position >= 0),
  kind TEXT NOT NULL CHECK(kind IN ('phase','log')),
  text TEXT NOT NULL,
  at INTEGER NOT NULL,
  PRIMARY KEY(run_id, position)
);
INSERT INTO kv(k, v) VALUES('schema', '8') ON CONFLICT(k) DO UPDATE SET v = '8';
COMMIT;
