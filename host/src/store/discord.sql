CREATE TABLE IF NOT EXISTS discord_inbox(
    event_id TEXT PRIMARY KEY,generation TEXT NOT NULL,bot_id TEXT NOT NULL,
    channel_id TEXT NOT NULL,entry_id TEXT UNIQUE NOT NULL,
    text TEXT NOT NULL,user_id TEXT NOT NULL,state TEXT NOT NULL DEFAULT 'queued'
    CHECK(state IN ('queued','started','rejected')),created_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS discord_threads(
    generation TEXT NOT NULL,channel_id TEXT NOT NULL,bot_id TEXT NOT NULL,
    root_id TEXT NOT NULL UNIQUE,message_id TEXT NOT NULL,
    PRIMARY KEY(generation,channel_id));
CREATE TABLE IF NOT EXISTS discord_outbox(
    entry_id TEXT PRIMARY KEY,state TEXT NOT NULL CHECK(state IN ('sending','sent','failed','unknown')),
    updated_at INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS discord_inbox_state ON discord_inbox(state,created_at);
