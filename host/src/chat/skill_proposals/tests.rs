//! The procedure test, the drafts, the inbox and its rules: pending only, edited, approved into a real skill of the bot, or
//! dismissed for good.

use super::*;
use crate::remote::identity::Identity;

fn rows(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter().map(|(k, t)| ((*k).to_owned(), (*t).to_owned())).collect()
}

#[test]
fn three_different_kinds_of_tool_are_a_procedure_and_three_reads_are_not() {
    let reads = tool_keys(&rows(&[("read", "Read a.rs"), ("read", "Read b.rs"), ("read", "Read c.rs")]));
    assert!(!worth_a_skill(&reads), "{reads:?}");
    let work = tool_keys(&rows(&[
        ("read", "Read a.rs"),
        ("edit", "Edit a.rs"),
        ("execute", "Run cargo test"),
        ("execute", "Run git status"),
    ]));
    assert_eq!(work.iter().cloned().collect::<Vec<_>>(), ["edit", "execute:cargo", "execute:git", "read"]);
    assert!(worth_a_skill(&work));
    // Thinking and the unknown say nothing about what was done.
    assert!(tool_keys(&rows(&[("think", "x"), ("other", "y"), ("", "z")])).is_empty());
}

#[test]
fn a_procedure_has_one_fingerprint_whatever_the_order_and_the_case() {
    let a = tool_keys(&rows(&[("read", "r"), ("edit", "e"), ("execute", "Run cargo build")]));
    let b = tool_keys(&rows(&[("execute", "Run Cargo test"), ("edit", "e"), ("read", "r")]));
    assert_eq!(fingerprint(&a, "Fix the BUILD please"), fingerprint(&b, "fix the build please"));
    assert_ne!(fingerprint(&a, "Fix the build"), fingerprint(&a, "Write the report"));
    let c = tool_keys(&rows(&[("read", "r"), ("edit", "e"), ("search", "s")]));
    assert_ne!(fingerprint(&a, "same"), fingerprint(&c, "same"));
}

#[test]
fn a_draft_is_read_from_the_models_reply_and_cut_to_the_limits() {
    let reply = "Sure!\n```json\n{\"name\": \"Ship a release\", \"when\": \"The user asks to publish a version.\", \"steps\": [\"Run the tests\", \" Tag it \", \"\"]}\n```";
    let (name, description, instructions) = parse_draft(reply).expect("a draft");
    assert_eq!((name.as_str(), description.as_str()), ("Ship a release", "The user asks to publish a version."));
    assert_eq!(instructions, "Use this when: The user asks to publish a version.\n\n1. Run the tests\n2. Tag it\n");
    for bad in [
        "no json here",
        "{\"name\": \"x\"}",
        "{\"name\": \"\", \"when\": \"w\", \"steps\": [\"s\"]}",
        "{\"name\": \"n\", \"when\": \"w\", \"steps\": []}",
        "{broken",
    ] {
        assert!(parse_draft(bad).is_none(), "{bad}");
    }
    let long = format!("{{\"name\": \"{}\", \"when\": \"w\", \"steps\": [\"s\"]}}", "n".repeat(500));
    assert_eq!(parse_draft(&long).expect("cut").0.chars().count(), MAX_NAME);
}

struct Fixture {
    hub: Arc<Hub>,
    bot: String,
    dir: std::path::PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn fixture() -> Fixture {
    let dir = std::env::temp_dir().join(format!("codync-proposals-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("dir");
    let store = Store::open(&dir.join("test.db")).expect("store");
    let hub =
        Hub::new(store, "proposals-test".into(), Identity::load_or_create(&dir).expect("identity"), "t".into(), 0);
    let bot = uuid::Uuid::new_v4().to_string();
    let cfg: BotConfig =
        serde_json::from_value(json!({"id": bot, "name": "Bot", "backend": "fixture", "command": "x", "cwd": dir}))
            .expect("config");
    hub.store.save_bot(&cfg).expect("bot");
    Fixture { hub, bot, dir }
}

fn proposal(f: &Fixture, id: &str, print: &str) -> Proposal {
    Proposal {
        id: id.into(),
        bot_id: f.bot.clone(),
        name: "Ship a release".into(),
        description: "When the user asks to publish.".into(),
        instructions: "Use this when: publishing\n\n1. Test\n2. Tag\n".into(),
        fingerprint: print.into(),
        tools: vec!["edit".into(), "execute:git".into(), "read".into()],
        created_at: 1,
    }
}

async fn ask(f: &Fixture, method: &str, extra: Value) -> Result<Value> {
    let mut body = json!({"botId": f.bot});
    body.as_object_mut().expect("object").extend(extra.as_object().cloned().unwrap_or_default());
    call(&f.hub, method, &body).await
}

#[tokio::test]
async fn proposals_wait_in_the_inbox_and_are_not_skills() {
    let f = fixture();
    save_pending(&f.hub.store, &f.bot, &[proposal(&f, "p1", "aaaa")]).expect("saved");
    let listed = ask(&f, "skillProposals", json!({})).await.expect("listed");
    assert_eq!(listed["proposals"].as_array().expect("list").len(), 1);
    assert_eq!(listed["proposals"][0]["name"], "Ship a release");
    let skills = crate::market::list_skills(&f.hub.store);
    assert!(!skills.to_string().contains("Ship a release"), "a proposal is not a skill yet");
}

#[tokio::test]
async fn an_edit_changes_the_draft_and_keeps_the_rest() {
    let f = fixture();
    save_pending(&f.hub.store, &f.bot, &[proposal(&f, "p1", "aaaa")]).expect("saved");
    let after = ask(&f, "editSkillProposal", json!({"id": "p1", "name": "  Publish  ", "instructions": ""}))
        .await
        .expect("edited");
    let first = &after["proposals"][0];
    assert_eq!(
        (first["name"].as_str(), first["description"].as_str()),
        (Some("Publish"), Some("When the user asks to publish."))
    );
    assert_eq!(
        first["instructions"], "Use this when: publishing\n\n1. Test\n2. Tag\n",
        "an empty field changes nothing"
    );
    assert!(ask(&f, "editSkillProposal", json!({"id": "missing", "name": "x"})).await.is_err());
}

#[tokio::test]
async fn a_dismissed_procedure_is_not_proposed_again() {
    let f = fixture();
    save_pending(&f.hub.store, &f.bot, &[proposal(&f, "p1", "aaaa"), proposal(&f, "p2", "bbbb")]).expect("saved");
    let after = ask(&f, "dismissSkillProposal", json!({"id": "p1"})).await.expect("dismissed");
    assert_eq!(after["proposals"].as_array().expect("list").len(), 1);
    assert!(dismissed(&f.hub.store, &f.bot).contains(&"aaaa".to_owned()));
    assert!(!dismissed(&f.hub.store, &f.bot).contains(&"bbbb".to_owned()));
    assert!(ask(&f, "dismissSkillProposal", json!({"id": "p1"})).await.is_err(), "it is gone");
}

#[tokio::test]
async fn approving_saves_a_skill_of_the_bot_and_empties_the_proposal() {
    let f = fixture();
    save_pending(&f.hub.store, &f.bot, &[proposal(&f, "p1", "aaaa")]).expect("saved");
    let after = ask(&f, "approveSkillProposal", json!({"id": "p1"})).await.expect("approved");
    assert_eq!(after["proposals"].as_array().expect("list").as_slice(), &[] as &[Value]);
    assert!(crate::market::list_skills(&f.hub.store).to_string().contains("Ship a release"), "now it is a skill");
    let notices: Vec<String> = f
        .hub
        .store
        .history(&f.bot, i64::MAX, 50)
        .expect("history")
        .into_iter()
        .filter(|e| e.kind == "notice")
        .filter_map(|e| e.data["text"].as_str().map(str::to_owned))
        .collect();
    assert!(notices.iter().any(|t| t.contains("Saved the skill")), "{notices:?}");
}

#[tokio::test]
async fn a_group_and_an_unknown_bot_have_no_inbox() {
    let f = fixture();
    assert!(call(&f.hub, "skillProposals", &json!({"botId": "nobody"})).await.is_err());
    assert!(call(&f.hub, "skillProposals", &json!({})).await.is_err());
}

#[test]
fn a_dismissed_list_is_capped_and_never_repeats_a_fingerprint() {
    let f = fixture();
    for n in 0..(MAX_DISMISSED + 10) {
        remember_dismissed(&f.hub.store, &f.bot, &format!("fp{n}")).expect("remembered");
    }
    remember_dismissed(&f.hub.store, &f.bot, "fp209").expect("again");
    let list = dismissed(&f.hub.store, &f.bot);
    assert_eq!(list.len(), MAX_DISMISSED);
    assert_eq!(list.iter().filter(|f| *f == "fp209").count(), 1);
    assert!(!list.contains(&"fp0".to_owned()), "the oldest went first");
}
