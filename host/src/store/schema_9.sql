BEGIN;
ALTER TABLE workflows ADD COLUMN owner_bot_id TEXT;
INSERT INTO kv(k, v) VALUES('schema', '10') ON CONFLICT(k) DO UPDATE SET v = '10';
COMMIT;
