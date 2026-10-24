use super::*;
use serde_json::json;

fn bot(id: &str, name: &str) -> BotConfig {
    serde_json::from_value(json!({"id": id, "name": name, "backend": "claude", "cwd": "/tmp"})).expect("valid bot")
}

fn said(text: &str) -> Entry {
    Entry {
        id: "e".into(),
        seq: 1,
        bot_id: "g".into(),
        rev: 1,
        kind: "user".into(),
        turn: 1,
        data: json!({"text": text}),
        created_at: 0,
        updated_at: 0,
    }
}

#[test]
fn mentions_pick_responders_and_no_mention_means_everyone() {
    let members = [bot("a", "Alice Chen"), bot("b", "Bob"), bot("c", "尼采課")];
    let ids = |m: &[BotConfig], text: &str| -> Vec<String> {
        responders(m, &[said(text)]).into_iter().map(|b| b.id.clone()).collect()
    };
    assert_eq!(ids(&members, "hi all"), ["a", "b", "c"]);
    assert_eq!(ids(&members, "@bob can you look"), ["b"]);
    assert_eq!(ids(&members, "@alicechen and @Alice"), ["a"]);
    assert_eq!(ids(&members, "@尼采課幫我整理"), ["c"], "CJK names can run into the message");
    assert_eq!(ids(&members, "@bobby?"), ["a", "b", "c"], "a longer word isn't a mention");
    assert_eq!(ids(&members, "@all go"), ["a", "b", "c"]);
    let mut reply = said("why?");
    reply.data["replyTo"] = json!({"id": "x", "author": "b", "text": "done"});
    let replied: Vec<String> = responders(&members, &[reply]).into_iter().map(|b| b.id.clone()).collect();
    assert_eq!(replied, ["b"], "a reply asks the bot it quotes");
}

#[test]
fn later_rounds_ask_only_the_members_another_member_mentioned() {
    let members = [bot("a", "Alice"), bot("b", "Bob"), bot("c", "Cara")];
    let by = |author: &str, text: &str, seq: i64| Entry {
        seq,
        kind: "agent".into(),
        data: json!({"text": text, "author": author}),
        ..said(text)
    };
    let asked_ids = |messages: &[Entry], asked: &[(&str, i64)]| -> Vec<String> {
        let asked = asked.iter().map(|(id, seq)| ((*id).to_owned(), *seq)).collect();
        cited(&members, messages, &asked).into_iter().map(|b| b.id.clone()).collect()
    };
    let ids = |messages: &[Entry]| asked_ids(messages, &[]);
    assert_eq!(ids(&[said("hi all"), by("a", "done", 2)]), Vec::<String>::new(), "nobody named: the room turn ends");
    let named = [said("hi"), by("a", "@bob check", 2)];
    assert_eq!(asked_ids(&named, &[("b", 2)]), Vec::<String>::new(), "Bob saw it and passed");
    assert_eq!(asked_ids(&named, &[("b", 1)]), ["b"], "Bob was asked before he was named");
    assert_eq!(ids(&[said("hi"), by("a", "@bob can you check?", 2)]), ["b"]);
    assert_eq!(
        ids(&[said("hi"), by("a", "@bob check", 2), by("b", "checked", 3)]),
        Vec::<String>::new(),
        "Bob already answered"
    );
    assert_eq!(ids(&[said("hi"), by("a", "@bob check", 2), by("b", "ok", 3), by("c", "@bob again", 4)]), ["b"]);
    assert_eq!(ids(&[said("hi"), by("a", "@alice @all over to you", 2)]), ["b", "c"], "not yourself");
    assert_eq!(
        ids(&[said("@cara look"), by("a", "ok", 2)]),
        Vec::<String>::new(),
        "the user's mentions pick the first round"
    );
}

use crate::api::devices::Caller;
use crate::hub::BotStatus;
use std::path::PathBuf;
use std::time::Duration;

/// A host with agent bots `a` and `b` (the scripted fixture agent) and a group of both.
struct Room {
    hub: Arc<Hub>,
    dir: PathBuf,
    group: String,
}

impl Room {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("codync-group-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir.join("test.db")).unwrap();
        let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/team_agent.py");
        for id in ["a", "b"] {
            let cwd = dir.join(id);
            std::fs::create_dir_all(&cwd).unwrap();
            let cfg: BotConfig = serde_json::from_value(json!({
                "id": id, "name": id, "backend": "fixture", "cwd": cwd,
                "command": crate::shell::python(&agent), "notify": false,
            }))
            .unwrap();
            store.save_bot(&cfg).unwrap();
        }
        let hub = Hub::new(
            store,
            "test-host".into(),
            crate::remote::identity::Identity::load_or_create(&dir).unwrap(),
            "test-token".into(),
            19222,
        );
        hub.start().unwrap();
        let group = hub
            .create_bot(
                serde_json::from_value(
                    json!({"id": "", "kind": "group", "name": "Crew", "description": "Ship the release", "members": ["a", "b", "a"]}),
                )
                .unwrap(),
            )
            .unwrap();
        let group = group["id"].as_str().unwrap().to_owned();
        Self { hub, dir, group }
    }

    async fn send(&self, text: &str, reply_to: Option<&str>) -> Entry {
        let sent = crate::api::dispatch(
            &self.hub,
            &Caller::Local,
            "send",
            json!({"botId": self.group, "text": text, "replyTo": reply_to}),
        )
        .await
        .unwrap();
        serde_json::from_value::<serde_json::Value>(sent["entry"].clone())
            .ok()
            .and_then(|e| self.hub.store.entry(e["id"].as_str()?))
            .unwrap()
    }

    fn replies(&self, lane: &Lane) -> Vec<(String, String)> {
        self.hub
            .store
            .messages_after(lane, 0, 100)
            .unwrap()
            .into_iter()
            .filter(|e| e.kind == "agent")
            .map(|e| (e.data["author"].as_str().unwrap().to_owned(), e.data["text"].as_str().unwrap().to_owned()))
            .collect()
    }

    async fn settle(&self) {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                tokio::time::sleep(Duration::from_millis(300)).await;
                let idle = ["a", "b"].iter().all(|b| self.hub.runtime(b).status == BotStatus::Idle);
                if idle && self.hub.bots_json(0).unwrap().iter().all(|b| b["status"] == "idle") {
                    break;
                }
            }
        })
        .await
        .expect("room settles");
    }

    async fn shutdown(self) {
        self.hub.shutdown().await;
        let _ = std::fs::remove_dir_all(self.dir);
    }
}

#[tokio::test]
async fn room_turns_follow_mentions_and_end_when_everyone_passes() {
    let room = Room::new();
    let row = room.hub.store.bot(&room.group).unwrap().unwrap();
    assert_eq!(row.config.members, ["a", "b"], "members are deduplicated");
    let again = room
        .hub
        .create_bot(
            serde_json::from_value(json!({"id": "", "kind": "group", "name": "Dup", "members": ["b", "a"]})).unwrap(),
        )
        .unwrap();
    assert_eq!(again["id"], room.group, "the same bots reopen their group");

    let main = Lane::main(&room.group);
    room.send("hello everyone", None).await;
    room.settle().await;
    // `a` answers the user; `b` then sees a bot spoke last and passes. Nobody named anyone: no second round.
    assert_eq!(room.replies(&main), [("a".to_owned(), "reply: group".to_owned())]);
    let prompts = |bot: &str| std::fs::read_to_string(room.dir.join(bot).join("prompts.jsonl")).unwrap_or_default();
    assert_eq!((prompts("a").lines().count(), prompts("b").lines().count()), (1, 1));

    room.send("@b your turn", None).await;
    room.settle().await;
    assert_eq!(room.replies(&main).last().unwrap().0, "b", "only the mentioned bot answers");
    assert_eq!(room.replies(&main).len(), 2);
    let listed = room.hub.bot_value_for_test(&room.group);
    assert_eq!(listed["lastMessage"], "b: reply: group");

    // A member brings a teammate into the next round by naming it.
    room.send("@a PING", None).await;
    room.settle().await;
    assert_eq!(room.replies(&main).last().unwrap(), &("a".to_owned(), "@b over to you".to_owned()));
    assert!(prompts("b").lines().last().unwrap().contains(r"a: @b over to you"), "b is asked in round two");
    assert_eq!(prompts("b").lines().count(), 3);
    let prompts = prompts("a");
    assert!(prompts.contains("Ship the release) with the user"), "members are told what the room is for: {prompts}");
    // The members' own chats stay private: the room's entries live in the group.
    assert!(room.hub.store.messages_after(&Lane::main("a"), 0, 100).unwrap().is_empty());
    room.shutdown().await;
}

#[tokio::test]
async fn a_skill_command_reaches_the_members_that_have_the_skill_on() {
    let room = Room::new();
    let pdf = json!([{"id": "pdf", "name": "PDF", "description": "", "source": "custom"}]);
    room.hub.store.kv_set("skills", &pdf.to_string()).unwrap();
    let mut a = room.hub.store.bot("a").unwrap().unwrap().config;
    a.skills = vec!["pdf".into()];
    room.hub.store.save_bot(&a).unwrap();

    room.send("/pdf merge these", None).await;
    room.settle().await;
    let prompts = |id: &str| std::fs::read_to_string(room.dir.join(id).join("prompts.jsonl")).unwrap();
    assert!(prompts("a").contains(r#"User: [Codync: the user picked the \"PDF\" skill"#), "{}", prompts("a"));
    assert!(
        prompts("b").contains("User: /pdf merge these"),
        "a member without it reads what was typed: {}",
        prompts("b")
    );
    room.shutdown().await;
}

#[tokio::test]
async fn replies_quote_the_message_they_answer() {
    let room = Room::new();
    let main = Lane::main(&room.group);
    room.send("@a start here", None).await;
    room.settle().await;
    let answer = room.hub.store.messages_after(&main, 0, 100).unwrap();
    let answer = answer.iter().find(|e| e.kind == "agent" && e.data["final"] == true).unwrap();
    let reply = room.send("@a about that", Some(&answer.id)).await;
    assert_eq!(reply.data["replyTo"], json!({"id": answer.id, "author": "a", "text": "reply: group"}));
    room.settle().await;
    let prompts = |id: &str| std::fs::read_to_string(room.dir.join(id).join("prompts.jsonl")).unwrap();
    assert!(
        prompts("a").contains(r#"User: [Codync: this replies to your message: \"reply: group\"] @a about that"#),
        "the room line carries the quote: {}",
        prompts("a")
    );
    let send = |b: serde_json::Value| crate::api::dispatch(&room.hub, &Caller::Local, "send", b);
    assert!(send(json!({"botId": "a", "text": "x", "replyTo": answer.id})).await.is_err(), "another chat's message");

    // Sending read the chat; only the answer to the reply is new.
    let unread = || room.hub.bot_value_for_test(&room.group)["unread"].as_i64().unwrap();
    assert_eq!(unread(), 1);
    crate::api::dispatch(&room.hub, &Caller::Local, "markRead", json!({"botId": room.group})).await.unwrap();
    assert_eq!(unread(), 0);

    // Reactions toggle, and reacting to a read message doesn't make it unread.
    let react = |emoji: &str| {
        crate::api::dispatch(&room.hub, &Caller::Local, "react", json!({"entryId": answer.id, "emoji": emoji}))
    };
    react("👍").await.unwrap();
    let reacted = react("✅").await.unwrap();
    assert_eq!(reacted["entry"]["data"]["reactions"], json!(["👍", "✅"]));
    assert_eq!(unread(), 0);
    let undone = react("👍").await.unwrap();
    assert_eq!(undone["entry"]["data"]["reactions"], json!(["✅"]));
    assert!(react("hi").await.is_err(), "only emoji");

    // In a bot's own chat the agent reads the quote before the text.
    let dm = send(json!({"botId": "b", "text": "main"})).await.unwrap();
    room.settle().await;
    send(json!({"botId": "b", "text": "aside", "replyTo": dm["entry"]["id"]})).await.unwrap();
    room.settle().await;
    assert!(
        prompts("b")
            .lines()
            .last()
            .unwrap()
            .contains(r#"[Codync: this replies to the user's earlier message: \"main\"]\naside"#),
        "{}",
        prompts("b")
    );
    room.shutdown().await;
}

#[tokio::test]
async fn stop_ends_the_room_turn_before_the_next_speaker() {
    let room = Room::new();
    room.send("BLOCK everyone look", None).await;
    let hub = room.hub.clone();
    tokio::time::timeout(Duration::from_secs(10), async {
        while hub.runtime("a").status != BotStatus::Working {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("a starts");
    crate::api::dispatch(&room.hub, &Caller::Local, "stop", json!({"botId": room.group})).await.unwrap();
    room.settle().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(!room.dir.join("b/prompts.jsonl").exists(), "b never got a turn after Stop");
    assert!(room.replies(&Lane::main(&room.group)).iter().all(|(_, t)| t != "reply: group"));
    room.shutdown().await;
}

/// `a` runs a turn in its own chat, and the room asks it: returns the room's notice.
async fn ask_busy_a(room: &Room) -> Entry {
    crate::api::dispatch(&room.hub, &Caller::Local, "send", json!({"botId": "a", "text": "BLOCK long work"}))
        .await
        .unwrap();
    let hub = room.hub.clone();
    tokio::time::timeout(Duration::from_secs(10), async {
        while hub.runtime("a").status != BotStatus::Working {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("a starts its own turn");

    room.send("@a are you there", None).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let entries = hub.store.history(&room.group, i64::MAX, 100).unwrap();
            if let Some(e) = entries.into_iter().find(|e| e.kind == "notice") {
                break e;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("a busy notice in the room")
}

#[tokio::test]
async fn a_member_busy_in_its_own_chat_is_announced_and_answers_once_free() {
    let room = Room::new();
    let main = Lane::main(&room.group);
    // The room says why `a` is quiet right away instead of waiting out the ask timeout in silence.
    let notice = ask_busy_a(&room).await;
    assert_eq!(notice.data["text"], "a is busy in another chat and will answer here when it's done.");
    assert_eq!(room.replies(&main), [] as [(String, String); 0]);

    // Its own turn ends, then it answers the room.
    std::fs::write(room.dir.join("a/release"), "").unwrap();
    room.settle().await;
    assert_eq!(room.replies(&main), [("a".to_owned(), "reply: group".to_owned())]);
    room.shutdown().await;
}

#[tokio::test]
async fn stopping_a_busy_member_in_its_own_chat_keeps_its_room_answer() {
    let room = Room::new();
    let main = Lane::main(&room.group);
    ask_busy_a(&room).await;
    // The user frees `a` from its own chat: the answer the room was promised still comes.
    crate::api::dispatch(&room.hub, &Caller::Local, "stop", json!({"botId": "a"})).await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while room.replies(&main).is_empty() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("a answers the room after its own Stop");
    assert_eq!(room.replies(&main), [("a".to_owned(), "reply: group".to_owned())]);
    room.shutdown().await;
}

#[tokio::test]
async fn a_late_answer_survives_newer_messages_and_sees_them() {
    let room = Room::new();
    let main = Lane::main(&room.group);
    ask_busy_a(&room).await;
    room.send("@b meanwhile check CI", None).await;
    tokio::time::timeout(Duration::from_secs(10), async {
        while room.replies(&main).is_empty() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("b answers while a is busy");

    std::fs::write(room.dir.join("a/release"), "").unwrap();
    room.settle().await;
    // `a` still takes its room turn, on a prompt built when it starts: it sees b's reply.
    let prompts = std::fs::read_to_string(room.dir.join("a/prompts.jsonl")).unwrap();
    let late = prompts.lines().last().unwrap();
    assert!(late.contains("User: @b meanwhile check CI") && late.contains("b: reply: group"), "{prompts}");
    room.shutdown().await;
}

#[test]
fn pass_is_recognized_loosely() {
    for t in ["(pass)", " (pass) ", "\"(pass)\"", "pass.", ""] {
        assert!(is_pass(t), "{t:?}");
    }
    assert!(!is_pass("I'll pass this to Bob"));
}
