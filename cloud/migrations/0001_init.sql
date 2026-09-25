CREATE TABLE accounts (
  user_id     TEXT PRIMARY KEY,                 -- Clerk sub
  email       TEXT,
  status      TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','deleted')),
  created_at  INTEGER NOT NULL,
  deleted_at  INTEGER
);

CREATE TABLE computers (
  id            TEXT PRIMARY KEY,               -- computerId
  sign_pub      TEXT NOT NULL UNIQUE,
  box_pub       TEXT NOT NULL,
  owner_user_id TEXT REFERENCES accounts(user_id),
  name          TEXT NOT NULL,
  platform      TEXT NOT NULL CHECK (platform IN ('macos','linux')),
  device        TEXT,                           -- laptop|macmini|…|linux（host 的 Device）
  version       TEXT NOT NULL,
  status        TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','blocked')),
  online        INTEGER NOT NULL DEFAULT 0,     -- DO 寫入
  last_seen_at  INTEGER,                        -- DO 寫入
  claimed_at    INTEGER,
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL
);
CREATE INDEX computers_owner ON computers(owner_user_id);

CREATE TABLE devices (
  id            TEXT PRIMARY KEY,               -- dev_…
  owner_user_id TEXT NOT NULL REFERENCES accounts(user_id),
  sign_pub      TEXT NOT NULL,
  name          TEXT NOT NULL,
  platform      TEXT NOT NULL CHECK (platform IN ('ios','macos')),
  created_at    INTEGER NOT NULL,
  last_used_at  INTEGER,
  revoked_at    INTEGER,
  UNIQUE (owner_user_id, sign_pub)
);

CREATE TABLE claims (
  id          TEXT PRIMARY KEY,                 -- clm_…
  user_id     TEXT NOT NULL REFERENCES accounts(user_id),
  nonce       TEXT NOT NULL,
  expires_at  INTEGER NOT NULL,
  consumed_at INTEGER,
  computer_id TEXT
);

CREATE TABLE access_requests (
  id          TEXT PRIMARY KEY,                 -- req_…
  computer_id TEXT NOT NULL REFERENCES computers(id),
  device_id   TEXT NOT NULL REFERENCES devices(id),
  user_id     TEXT NOT NULL REFERENCES accounts(user_id),
  status      TEXT NOT NULL CHECK (status IN ('pending','approved','denied','expired','cancelled')),
  commit_hash  TEXT NOT NULL,                   -- SHA-256("codync/sascommit/v1" ‖ dk ‖ nD)
  host_nonce   TEXT,                            -- nH，host 設一次
  device_nonce TEXT,                            -- nD，host_nonce 存在後裝置才可設
  created_at  INTEGER NOT NULL,
  expires_at  INTEGER NOT NULL,                 -- created_at + 10 min
  decided_at  INTEGER,
  grant_id    TEXT
);
CREATE INDEX access_requests_computer ON access_requests(computer_id, status);
CREATE UNIQUE INDEX access_requests_one_pending ON access_requests(computer_id, device_id) WHERE status = 'pending';

CREATE TABLE grants (
  id             TEXT PRIMARY KEY,              -- grt_…
  computer_id    TEXT NOT NULL REFERENCES computers(id),
  device_id      TEXT NOT NULL REFERENCES devices(id),
  scopes         TEXT NOT NULL,                 -- JSON array
  status         TEXT NOT NULL CHECK (status IN ('active','revoked')),
  created_at     INTEGER NOT NULL,
  revoked_at     INTEGER,
  revoked_reason TEXT                           -- owner|deviceRevoked|unclaimed|accountDeleted
);
CREATE UNIQUE INDEX grants_active ON grants(computer_id, device_id) WHERE status = 'active';

CREATE TABLE sig_nonces (
  kid        TEXT NOT NULL,
  nonce      TEXT NOT NULL,
  expires_at INTEGER NOT NULL,
  PRIMARY KEY (kid, nonce)
);

CREATE TABLE processed_events (
  provider     TEXT NOT NULL,
  event_id     TEXT NOT NULL,
  processed_at INTEGER NOT NULL,
  PRIMARY KEY (provider, event_id)
);

CREATE TABLE audit_events (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  actor      TEXT NOT NULL,                     -- user_id / computer:<id> / system
  action     TEXT NOT NULL,
  target     TEXT,
  result     TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE INDEX audit_time ON audit_events(created_at);
