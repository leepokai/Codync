CREATE TABLE IF NOT EXISTS whatsapp_threads(
    chat TEXT NOT NULL, bot_id TEXT NOT NULL, root_id TEXT UNIQUE NOT NULL,
    sender TEXT NOT NULL DEFAULT '', PRIMARY KEY(chat, bot_id));
CREATE TABLE IF NOT EXISTS whatsapp_outbox(
    entry_id TEXT PRIMARY KEY, state TEXT NOT NULL CHECK(state IN ('sending','sent','failed')),
    updated_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS whatsapp_cards(
    chat TEXT NOT NULL, entry_id TEXT NOT NULL, sender TEXT NOT NULL DEFAULT '', sent_at INTEGER NOT NULL,
    PRIMARY KEY(chat, entry_id));
CREATE INDEX IF NOT EXISTS whatsapp_cards_chat ON whatsapp_cards(chat, sent_at DESC);
CREATE TABLE IF NOT EXISTS whatsapp_card_exclusions(entry_id TEXT PRIMARY KEY);
