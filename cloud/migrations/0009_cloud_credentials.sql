-- Codync Cloud on the user's own provider account (docs/plans/codync-cloud.md, "Bring your own cloud"): the API key
-- each user pastes once, sealed with the Worker secret CLOUD_SECRET_KEYS (src/secrets.ts). Only `hint` is ever shown.
CREATE TABLE cloud_credentials (
  user_id     TEXT NOT NULL REFERENCES accounts(user_id),
  provider    TEXT NOT NULL CHECK (provider IN ('boat')),
  sealed      TEXT NOT NULL,                    -- IV + AES-256-GCM ciphertext, bound to user_id and provider
  key_id      TEXT NOT NULL,                    -- which CLOUD_SECRET_KEYS entry sealed it
  hint        TEXT NOT NULL,                    -- boat_…9abc
  problem     TEXT,                             -- the provider's code when it last turned the key down
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  PRIMARY KEY (user_id, provider)
);
