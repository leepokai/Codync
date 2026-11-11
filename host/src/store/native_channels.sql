CREATE TABLE IF NOT EXISTS native_channel_inbox(
 connection TEXT NOT NULL,generation TEXT NOT NULL,event TEXT NOT NULL,bot TEXT NOT NULL,
 payload TEXT NOT NULL,entry TEXT,state TEXT NOT NULL DEFAULT 'queued' CHECK(state IN('queued','started','rejected')),
 created_at INTEGER NOT NULL,PRIMARY KEY(connection,generation,event));
CREATE INDEX IF NOT EXISTS native_channel_queue ON native_channel_inbox(connection,state);
CREATE TABLE IF NOT EXISTS native_channel_routes(
 root TEXT PRIMARY KEY,connection TEXT NOT NULL,generation TEXT NOT NULL,bot TEXT NOT NULL,
 chat TEXT NOT NULL,user TEXT NOT NULL,reply_to TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS native_channel_outbox(
 entry TEXT PRIMARY KEY,connection TEXT NOT NULL,generation TEXT NOT NULL,payload TEXT NOT NULL,
 code TEXT,external_id TEXT,state TEXT NOT NULL CHECK(state IN('sending','sent','unknown','app_only')),
 updated_at INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS native_channel_cards ON native_channel_outbox(connection,generation,code);
CREATE TABLE IF NOT EXISTS native_channel_users(
 connection TEXT NOT NULL,generation TEXT NOT NULL,user TEXT NOT NULL,allowed INTEGER NOT NULL CHECK(allowed IN(0,1)),
 PRIMARY KEY(connection,generation,user));
CREATE TABLE IF NOT EXISTS native_channel_pairings(
 connection TEXT PRIMARY KEY,generation TEXT NOT NULL,hash TEXT NOT NULL,expires INTEGER NOT NULL,attempts INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS native_channel_webhooks(
 id TEXT PRIMARY KEY,connection TEXT NOT NULL,generation TEXT NOT NULL,raw TEXT NOT NULL,created_at INTEGER NOT NULL,
 state TEXT NOT NULL DEFAULT 'queued' CHECK(state IN('queued','claimed','done')));
