-- Codync Cloud on the user's own provider account (docs/plans/codync-cloud.md, "Bring your own cloud"): the API key
-- each user pastes once, sealed with the Worker secret CLOUD_SECRET_KEYS (src/secrets.ts). Only `hint` is ever shown.
-- Which providers exist is the Worker's to say (`drivers`, `PROVIDERS`), so no CHECK names them here.
CREATE TABLE cloud_credentials (
  user_id     TEXT NOT NULL REFERENCES accounts(user_id),
  provider    TEXT NOT NULL,
  sealed      TEXT NOT NULL,                    -- IV + AES-256-GCM ciphertext, bound to user_id and provider
  key_id      TEXT NOT NULL,                    -- which CLOUD_SECRET_KEYS entry sealed it
  hint        TEXT NOT NULL,                    -- boat_…9abc
  problem     TEXT,                             -- why the provider last turned the key down (a Failure, admin, format)
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  PRIMARY KEY (user_id, provider)
);

-- cloud_machines again, without its provider CHECK and its one-per-account index, and with `name` (the user's name for
-- it, which its host takes too) and setup_at: when a machine got its claim ticket and setup script (with the create, or
-- once it ran, for providers that take nothing at create). Earlier machines got both at create. vault_*: the key to
-- the machine's keeper vault (sealed like the credentials), handed only to the keeper whose key (keeper_key) its claim
-- named, and erased with the machine: the disk holds only ciphertext, though a copy of it can fetch the key until then.
CREATE TABLE cloud_machines_new (
  id             TEXT PRIMARY KEY,                 -- cm_…
  user_id        TEXT NOT NULL REFERENCES accounts(user_id),
  name           TEXT NOT NULL,
  provider       TEXT NOT NULL,
  provider_id    TEXT NOT NULL,                    -- the provider's machine id
  claim_id       TEXT NOT NULL,
  computer_id    TEXT,                             -- set when the host claims itself
  state          TEXT NOT NULL CHECK (state IN ('starting','running','stopping','stopped','failed','deleted')),
  backup         TEXT,                             -- name of the never-expiring backup, for providers whose stopped disk expires
  backup_at      INTEGER,
  setup_at       INTEGER,
  vault_sealed   TEXT,
  vault_key_id   TEXT,
  keeper_key     TEXT,                             -- the keeper's Ed25519 key, base64url (set once, at the claim)
  created_at     INTEGER NOT NULL,
  updated_at     INTEGER NOT NULL
);
INSERT INTO cloud_machines_new (id, user_id, name, provider, provider_id, claim_id, computer_id, state, backup, backup_at, setup_at, created_at, updated_at)
  SELECT id, user_id, 'Codync Cloud', provider, provider_id, claim_id, computer_id, state, backup, backup_at, created_at, created_at, updated_at FROM cloud_machines;
DROP TABLE cloud_machines;
ALTER TABLE cloud_machines_new RENAME TO cloud_machines;
CREATE INDEX cloud_machines_user ON cloud_machines(user_id);
CREATE INDEX cloud_machines_computer ON cloud_machines(computer_id);
