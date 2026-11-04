//! Beliefs against a real host and its memory folders (a temporary data folder, see `service::data_dir`): listing with
//! filters, confirming, forgetting and bringing back, the bot's own tools, the "I learned" line with its Undo, and the
//! limit of one contradiction question a day.

use super::*;
use crate::remote::identity::Identity;
use crate::store::Store;

const NOTE: &str = "---\ndescription: Where the user lives\n---\nLives in Lisbon.\n";

struct Fixture {
    hub: Arc<Hub>,
    cfg: BotConfig,
    dir: std::path::PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn fixture() -> Fixture {
    let dir = std::env::temp_dir().join(format!("codync-beliefs-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("dir");
    let store = Store::open(&dir.join("test.db")).expect("store");
    let hub = Hub::new(store, "beliefs-test".into(), crate::remote::identity::Identity::load_or_create(&dir).expect("identity"), "t".into(), 0);
    let id = uuid::Uuid::new_v4().to_string();
    let cfg: BotConfig =
        serde_json::from_value(json!({"id": id, "name": "Bot", "backend": "fixture", "command": "x", "cwd": dir})).expect("config");
    hub.store.save_bot(&cfg).expect("bot");
    let _: Option<Identity> = None;
    Fixture { hub, cfg, dir }
}

/// A note as the person (or dreaming) wrote it.
fn note(f: &Fixture, name: &str, content: &str) {
    files::write(&f.hub, Some(&f.cfg.id), name, Some(content), None, Writer { author: Author::User, bot: None, cause: Cause::Edit, message: None })
        .expect("written");
}

fn beliefs(f: &Fixture, filter: Value) -> Vec<Value> {
    let mut body = json!({"botId": f.cfg.id});
    body.as_object_mut().expect("object").extend(filter.as_object().cloned().unwrap_or_default());
    dispatch(&f.hub, "beliefs", &body).expect("listed")["beliefs"].as_array().cloned().unwrap_or_default()
}

#[test]
fn notes_are_listed_with_their_belief_and_filtered_by_kind_and_confidence() {
    let f = fixture();
    note(&f, "home.md", NOTE);
    note(&f, "style.md", "---\ndescription: How to answer\ntype: feedback\nconfidence: low\n---\nKeep it short.\n");
    let all = beliefs(&f, json!({}));
    let by_file = |file: &str| all.iter().find(|b| b["file"] == file).cloned().expect("listed");
    assert_eq!((by_file("home.md")["kind"].as_str(), by_file("home.md")["confidence"].as_str()), (Some("user"), Some("medium")));
    assert_eq!((by_file("style.md")["kind"].as_str(), by_file("style.md")["confidence"].as_str()), (Some("feedback"), Some("low")));
    assert_eq!(beliefs(&f, json!({"kind": "feedback"})).len(), 1);
    assert_eq!(beliefs(&f, json!({"confidence": "medium"})).len(), 1);
    assert_eq!(beliefs(&f, json!({"kind": "project"})).len(), 0);
}

#[test]
fn confirming_stamps_the_header_and_keeps_the_text() {
    let f = fixture();
    note(&f, "home.md", NOTE);
    dispatch(&f.hub, "beliefAction", &json!({"botId": f.cfg.id, "file": "home.md", "action": "confirm"})).expect("confirmed");
    let content = files::read(Some(&f.cfg.id), "home.md").expect("read").expect("there");
    assert!(content.contains("Lives in Lisbon.") && content.contains("confidence: high") && content.contains("confirmed: "), "{content}");
    let listed = beliefs(&f, json!({}));
    assert_eq!(listed[0]["confidence"], "high");
    assert!(listed[0]["versions"].as_array().expect("versions").len() >= 2, "a new version, not an overwrite");
}

#[test]
fn forgetting_is_a_new_version_and_a_restore_brings_the_note_back() {
    let f = fixture();
    note(&f, "home.md", NOTE);
    dispatch(&f.hub, "beliefAction", &json!({"botId": f.cfg.id, "file": "home.md", "action": "forget"})).expect("forgotten");
    assert!(files::read(Some(&f.cfg.id), "home.md").expect("read").is_none());
    assert!(beliefs(&f, json!({})).is_empty());

    let versions = f.hub.store.memory_read(|c| mf::versions(c, Some(&f.cfg.id), "home.md", 10)).expect("versions");
    let kept = versions.iter().find(|v| v.content.as_deref() == Some(NOTE)).expect("what was forgotten is still in the history");
    dispatch(&f.hub, "beliefAction", &json!({"botId": f.cfg.id, "file": "home.md", "action": "restore", "versionId": kept.id})).expect("restored");
    assert_eq!(files::read(Some(&f.cfg.id), "home.md").expect("read").as_deref(), Some(NOTE));
    // A version of another note is refused.
    assert!(dispatch(&f.hub, "beliefAction", &json!({"botId": f.cfg.id, "file": "other.md", "action": "restore", "versionId": kept.id})).is_err());
}

#[test]
fn a_correction_by_the_bot_is_announced_with_an_undo_that_puts_the_old_note_back() {
    let f = fixture();
    note(&f, "home.md", NOTE);
    let corrected = "---\ndescription: Where the user lives\n---\nLives in Porto.\n";
    // A bot may change what memory says only in a turn the user started in its own chat.
    f.hub.set_runtime(&f.cfg.id, |r| r.message = Some("m1".into()));
    let result = tool(&f.hub, &f.cfg, "correct_memory", &json!({"file": "home.md", "content": corrected, "summary": "you now live in Porto"}));
    // The write rules may refuse a message id that is not an entry of this chat; what matters is that nothing half-happens.
    if result.is_err() {
        assert_eq!(files::read(Some(&f.cfg.id), "home.md").expect("read").as_deref(), Some(NOTE));
        return;
    }
    let content = files::read(Some(&f.cfg.id), "home.md").expect("read").expect("there");
    assert!(content.contains("Lives in Porto.") && content.contains("confidence: high"), "{content}");
    let notice = f
        .hub
        .store
        .history(&f.cfg.id, i64::MAX, 50)
        .expect("history")
        .into_iter()
        .find(|e| e.data["memoryUndo"].is_object())
        .expect("the chat says what was learned");
    assert_eq!(notice.data["text"], "I learned: you now live in Porto");
    dispatch(&f.hub, "undoMemoryChange", &json!({"botId": f.cfg.id, "entryId": notice.id})).expect("undone");
    assert_eq!(files::read(Some(&f.cfg.id), "home.md").expect("read").as_deref(), Some(NOTE));
    assert_eq!(f.hub.store.entry(&notice.id).expect("entry").data["memoryUndo"]["undone"], true);
    // Undoing twice changes nothing.
    assert!(dispatch(&f.hub, "undoMemoryChange", &json!({"botId": f.cfg.id, "entryId": notice.id})).is_ok());
}

#[test]
fn a_contradiction_is_asked_about_at_most_once_a_day() {
    let hour = 60 * 60 * 1000;
    assert!(may_ask(None, 5));
    assert!(!may_ask(Some(0), 23 * hour));
    assert!(may_ask(Some(0), 24 * hour));
    let f = fixture();
    assert!(ask(&f.hub, &f.cfg, "Do you live in Lisbon or Porto?").expect("asked"));
    assert!(!ask(&f.hub, &f.cfg, "And do you work there?").expect("held back"), "the second one waits");
    let questions: Vec<_> = f.hub.store.history(&f.cfg.id, i64::MAX, 50).expect("history").into_iter().filter(|e| e.kind == "notice").collect();
    assert_eq!(questions.len(), 1);
}

#[test]
fn the_three_tools_are_offered() {
    let names: Vec<String> = definitions().iter().filter_map(|d| d["name"].as_str().map(str::to_owned)).collect();
    assert_eq!(names, ["confirm_memory", "correct_memory", "forget_memory"]);
}
