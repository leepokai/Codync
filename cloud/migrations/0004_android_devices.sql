-- Keep IDs and authorization history. Rebuild the two referencing tables too:
-- dropping/replacing a populated parent alone leaves D1's deferred FK counter
-- violated even when the replacement has the same IDs.
CREATE TABLE devices_android (
  id            TEXT PRIMARY KEY,
  owner_user_id TEXT NOT NULL REFERENCES accounts(user_id),
  sign_pub      TEXT NOT NULL,
  name          TEXT NOT NULL,
  platform      TEXT NOT NULL CHECK (platform IN ('ios','macos','linux','windows','android')),
  created_at    INTEGER NOT NULL,
  last_used_at  INTEGER,
  revoked_at    INTEGER,
  UNIQUE (owner_user_id, sign_pub)
);
INSERT INTO devices_android SELECT id, owner_user_id, sign_pub, name, platform, created_at, last_used_at, revoked_at FROM devices;
CREATE TABLE grants_android (
  id             TEXT PRIMARY KEY,
  computer_id    TEXT NOT NULL REFERENCES computers(id),
  device_id      TEXT NOT NULL REFERENCES devices_android(id),
  scopes         TEXT NOT NULL,
  status         TEXT NOT NULL CHECK (status IN ('active','revoked')),
  created_at     INTEGER NOT NULL,
  revoked_at     INTEGER,
  revoked_reason TEXT
);
INSERT INTO grants_android SELECT * FROM grants;
CREATE TABLE access_requests_android (
  id           TEXT PRIMARY KEY,
  computer_id  TEXT NOT NULL REFERENCES computers(id),
  device_id    TEXT NOT NULL REFERENCES devices_android(id),
  user_id      TEXT NOT NULL REFERENCES accounts(user_id),
  status       TEXT NOT NULL CHECK (status IN ('pending','approved','denied','expired','cancelled')),
  commit_hash  TEXT NOT NULL,
  host_nonce   TEXT,
  device_nonce TEXT,
  created_at   INTEGER NOT NULL,
  expires_at   INTEGER NOT NULL,
  decided_at   INTEGER,
  grant_id     TEXT
);
INSERT INTO access_requests_android SELECT * FROM access_requests;
DROP TABLE grants;
DROP TABLE access_requests;
DROP TABLE devices;
ALTER TABLE devices_android RENAME TO devices;
ALTER TABLE grants_android RENAME TO grants;
ALTER TABLE access_requests_android RENAME TO access_requests;
CREATE UNIQUE INDEX grants_active ON grants(computer_id, device_id) WHERE status = 'active';
CREATE INDEX access_requests_computer ON access_requests(computer_id, status);
CREATE UNIQUE INDEX access_requests_one_pending ON access_requests(computer_id, device_id) WHERE status = 'pending';
