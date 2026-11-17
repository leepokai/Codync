-- Cloud computers Codync runs for signed-in users (docs/plans/codync-cloud.md). The machine claims
-- itself to the account with claim_id on its first boot; claims rows are swept a day after they expire,
-- so claim_id is not a foreign key.
CREATE TABLE cloud_machines (
  id             TEXT PRIMARY KEY,                 -- cm_…
  user_id        TEXT NOT NULL REFERENCES accounts(user_id),
  provider       TEXT NOT NULL CHECK (provider IN ('boat')),
  provider_id    TEXT NOT NULL,                    -- the provider's machine id
  claim_id       TEXT NOT NULL,
  computer_id    TEXT,                             -- set when the host claims itself
  state          TEXT NOT NULL CHECK (state IN ('starting','running','stopping','stopped','failed','deleted')),
  backup         TEXT,                             -- name of the never-expiring backup, saved after 10 days asleep
  backup_at      INTEGER,
  created_at     INTEGER NOT NULL,
  updated_at     INTEGER NOT NULL
);
CREATE UNIQUE INDEX cloud_machines_one_per_user ON cloud_machines(user_id) WHERE state != 'deleted';
CREATE INDEX cloud_machines_computer ON cloud_machines(computer_id);
