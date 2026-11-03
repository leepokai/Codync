-- Short pairing codes (docs/plans/require-sign-in.md, phase 3): `codync-host pair` registers its QR link under a
-- code people can type, like QRTX-4821. One use, ten minutes.
CREATE TABLE pairing_codes (
  code        TEXT PRIMARY KEY,
  computer_id TEXT NOT NULL REFERENCES computers(id),
  link        TEXT NOT NULL,
  expires_at  INTEGER NOT NULL,
  used_at     INTEGER
);
CREATE INDEX pairing_codes_computer ON pairing_codes(computer_id);
