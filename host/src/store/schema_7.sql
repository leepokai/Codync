BEGIN;
CREATE TABLE workflows (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL CHECK(length(title) BETWEEN 1 AND 160),
  revision INTEGER NOT NULL DEFAULT 0 CHECK(revision >= 0),
  enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0, 1)),
  strokes_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(strokes_json)),
  next_run_number INTEGER NOT NULL DEFAULT 1 CHECK(next_run_number >= 1),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE TABLE workflow_nodes (
  workflow_id TEXT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
  node_id TEXT NOT NULL,
  position INTEGER NOT NULL CHECK(position >= 0),
  kind TEXT NOT NULL CHECK(kind IN (
    'trigger','prompt','agent','condition','approval','output','text','note','shape')),
  bot_id TEXT,
  x REAL NOT NULL,
  y REAL NOT NULL,
  width REAL,
  height REAL,
  locked INTEGER NOT NULL DEFAULT 0 CHECK(locked IN (0, 1)),
  fields_json TEXT NOT NULL CHECK(json_valid(fields_json)),
  schedule_json TEXT CHECK(schedule_json IS NULL OR json_valid(schedule_json)),
  PRIMARY KEY(workflow_id, node_id)
);
CREATE INDEX workflow_nodes_bot ON workflow_nodes(bot_id) WHERE bot_id IS NOT NULL;
CREATE TABLE workflow_edges (
  workflow_id TEXT NOT NULL,
  edge_id TEXT NOT NULL,
  position INTEGER NOT NULL CHECK(position >= 0),
  from_node_id TEXT NOT NULL,
  to_node_id TEXT NOT NULL,
  port_type TEXT NOT NULL CHECK(port_type IN ('text')),
  branch TEXT CHECK(branch IS NULL OR branch IN ('yes','no')),
  CHECK(from_node_id <> to_node_id),
  PRIMARY KEY(workflow_id, edge_id),
  FOREIGN KEY(workflow_id, from_node_id) REFERENCES workflow_nodes(workflow_id, node_id) ON DELETE CASCADE,
  FOREIGN KEY(workflow_id, to_node_id) REFERENCES workflow_nodes(workflow_id, node_id) ON DELETE CASCADE
);
CREATE INDEX workflow_edges_from ON workflow_edges(workflow_id, from_node_id);
CREATE INDEX workflow_edges_to ON workflow_edges(workflow_id, to_node_id);
CREATE TABLE workflow_versions (
  id TEXT PRIMARY KEY,
  workflow_id TEXT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
  revision INTEGER NOT NULL CHECK(revision >= 0),
  kind TEXT NOT NULL CHECK(kind IN ('auto','restore','run')),
  graph_json TEXT NOT NULL CHECK(json_valid(graph_json)),
  created_at INTEGER NOT NULL
);
CREATE INDEX workflow_versions_recent ON workflow_versions(workflow_id, created_at DESC, id);
CREATE TABLE workflow_triggers (
  workflow_id TEXT NOT NULL,
  node_id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('manual','schedule','webhook')),
  timezone TEXT,
  schedule_json TEXT CHECK(schedule_json IS NULL OR json_valid(schedule_json)),
  next_due_at INTEGER,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY(workflow_id, node_id),
  FOREIGN KEY(workflow_id, node_id) REFERENCES workflow_nodes(workflow_id, node_id) ON DELETE CASCADE
);
CREATE INDEX workflow_triggers_due ON workflow_triggers(next_due_at) WHERE next_due_at IS NOT NULL;
CREATE TABLE workflow_webhook_secrets (
  workflow_id TEXT NOT NULL,
  node_id TEXT NOT NULL,
  hook_id TEXT NOT NULL UNIQUE,
  secret TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY(workflow_id, node_id),
  FOREIGN KEY(workflow_id, node_id) REFERENCES workflow_triggers(workflow_id, node_id) ON DELETE CASCADE
);
CREATE TABLE workflow_webhook_receipts (
  id TEXT PRIMARY KEY,
  workflow_id TEXT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
  node_id TEXT NOT NULL,
  delivery_id TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('started','ignored')),
  reason TEXT CHECK(reason IS NULL OR reason IN ('busy','refused')),
  run_id TEXT,
  received_at INTEGER NOT NULL,
  UNIQUE(workflow_id, node_id, delivery_id)
);
CREATE TABLE workflow_runs (
  id TEXT PRIMARY KEY,
  workflow_id TEXT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
  number INTEGER NOT NULL CHECK(number >= 1),
  trigger_node_id TEXT,
  from_node_id TEXT,
  source TEXT NOT NULL CHECK(source IN ('manual','schedule','webhook')),
  scheduled_for INTEGER,
  idempotency_key TEXT NOT NULL,
  revision INTEGER NOT NULL CHECK(revision >= 0),
  graph_json TEXT NOT NULL CHECK(json_valid(graph_json)),
  payload TEXT,
  status TEXT NOT NULL CHECK(status IN ('running','approval','stopping','done','failed','stopped')),
  error TEXT,
  started_at INTEGER NOT NULL,
  ended_at INTEGER,
  updated_at INTEGER NOT NULL,
  UNIQUE(workflow_id, number),
  UNIQUE(workflow_id, idempotency_key)
);
CREATE INDEX workflow_runs_recent ON workflow_runs(workflow_id, started_at DESC, id);
CREATE INDEX workflow_runs_open ON workflow_runs(status) WHERE status IN ('running','approval','stopping');
CREATE TABLE workflow_steps (
  id TEXT PRIMARY KEY,
  workflow_id TEXT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
  run_id TEXT REFERENCES workflow_runs(id) ON DELETE CASCADE,
  node_id TEXT NOT NULL,
  attempt INTEGER NOT NULL CHECK(attempt >= 1),
  request_id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN (
    'prompt','agent','condition','approval','output','trigger')),
  bot_id TEXT,
  status TEXT NOT NULL CHECK(status IN ('running','approval','done','failed','stopped','skipped')),
  input TEXT NOT NULL DEFAULT '',
  output TEXT,
  branch TEXT CHECK(branch IS NULL OR branch IN ('yes','no')),
  passes INTEGER NOT NULL DEFAULT 1 CHECK(passes >= 1),
  truncated INTEGER NOT NULL DEFAULT 0 CHECK(truncated IN (0, 1)),
  error TEXT,
  started_at INTEGER,
  ended_at INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(workflow_id, request_id)
);
CREATE UNIQUE INDEX workflow_steps_attempt
  ON workflow_steps(run_id, node_id, attempt) WHERE run_id IS NOT NULL;
CREATE INDEX workflow_steps_open ON workflow_steps(status) WHERE status IN ('running','approval');
INSERT INTO kv(k, v) VALUES('schema', '7') ON CONFLICT(k) DO UPDATE SET v = '7';
COMMIT;
