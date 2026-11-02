BEGIN;
ALTER TABLE workflows ADD COLUMN owner_bot_id TEXT;
INSERT INTO kv(k, v) VALUES('schema', '11') ON CONFLICT(k) DO UPDATE SET v = '11';
COMMIT;
