use super::*;
use crate::store::{EntryKind, Lane};
use serde_json::json;

#[test]
fn whatsapp_conversations_and_senders_are_isolated_by_bot() {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    for (bot, root, sender) in [("a", "root-a", "alice"), ("b", "root-b", "bob")] {
        store.insert_entry_once(root, &Lane::main(bot), EntryKind::User, 0, &json!({"text":"root"})).unwrap();
        store.map_whatsapp_thread("chat", bot, root, sender).unwrap();
    }
    assert_eq!(store.whatsapp_root("chat", "a").unwrap().as_deref(), Some("root-a"));
    assert_eq!(store.whatsapp_root("chat", "b").unwrap().as_deref(), Some("root-b"));
    for (bot, root) in [("a", "root-a"), ("b", "root-b")] {
        store
            .insert_entry(&Lane::in_thread(bot, root), EntryKind::Agent, 1, &json!({"text":bot,"final":true}))
            .unwrap();
    }
    let replies = store.pending_whatsapp_replies().unwrap();
    assert_eq!(replies.len(), 2);
    assert!(replies.iter().all(|reply| reply.chat == "chat"));
}

#[test]
fn whatsapp_migration_retains_existing_roots_and_approval_receipts() {
    let db = rusqlite::Connection::open_in_memory().unwrap();
    db.execute_batch(
        "CREATE TABLE entries(id TEXT PRIMARY KEY,bot_id TEXT);
        INSERT INTO entries VALUES('old-root','a');
        CREATE TABLE whatsapp_threads(chat TEXT PRIMARY KEY,root_id TEXT UNIQUE NOT NULL,sender TEXT NOT NULL);
        INSERT INTO whatsapp_threads VALUES('chat','old-root','alice');",
    )
    .unwrap();
    open(&db).unwrap();
    open(&db).unwrap();
    assert_eq!(
        db.query_row("SELECT root_id FROM whatsapp_threads WHERE chat='chat' AND bot_id='a'", [], |row| row
            .get::<_, String>(0))
            .unwrap(),
        "old-root"
    );
    db.execute("INSERT INTO whatsapp_threads VALUES('chat','b','new-root','bob')", []).unwrap();
    assert_eq!(db.query_row("SELECT count(*) FROM whatsapp_threads", [], |row| row.get::<_, i64>(0)).unwrap(), 2);
}

#[test]
fn app_only_decisions_do_not_starve_later_whatsapp_cards_or_grant_reply_rights() {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    store.map_whatsapp_thread("chat", "bot", "root", "alice").unwrap();
    for index in 0..11 {
        let entry = store
            .insert_entry(&Lane::in_thread("bot", "root"), EntryKind::Permission, 1, &json!({"status":"pending"}))
            .unwrap();
        if index < 10 {
            store.exclude_whatsapp_card(&entry.id).unwrap();
        }
    }
    assert_eq!(store.pending_whatsapp_cards().unwrap().len(), 1);
    assert!(store.whatsapp_card("chat", "alice").is_none());
}

#[test]
fn whatsapp_migration_reads_the_oldest_table_that_had_no_sender() {
    let db = rusqlite::Connection::open_in_memory().unwrap();
    db.execute_batch(
        "CREATE TABLE entries(id TEXT PRIMARY KEY,bot_id TEXT);
        INSERT INTO entries VALUES('old-root','a');
        CREATE TABLE whatsapp_threads(chat TEXT PRIMARY KEY,root_id TEXT UNIQUE NOT NULL);
        INSERT INTO whatsapp_threads VALUES('chat','old-root');",
    )
    .unwrap();
    open(&db).unwrap();
    let (root, sender): (String, String) = db
        .query_row("SELECT root_id,sender FROM whatsapp_threads WHERE chat='chat' AND bot_id='a'", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    assert_eq!((root.as_str(), sender.as_str()), ("old-root", ""));
}
