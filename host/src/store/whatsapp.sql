CREATE TABLE IF NOT EXISTS whatsapp_threads(chat TEXT PRIMARY KEY, root_id TEXT UNIQUE NOT NULL);
CREATE TABLE IF NOT EXISTS whatsapp_outbox(
    entry_id TEXT PRIMARY KEY, state TEXT NOT NULL CHECK(state IN ('sending','sent','failed')),
    updated_at INTEGER NOT NULL);
