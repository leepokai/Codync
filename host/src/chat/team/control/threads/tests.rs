#![allow(clippy::unwrap_used)]
use super::super::{call, clients};
use super::*;
use crate::chat::calls::context::tests::{bot, hub};
use crate::store::{EntryKind, Lane};
use codync_network::Access;

fn app(id: &str, level: Level, scopes: &[Scope]) -> Principal {
    Principal {
        id: id.into(),
        label: format!("App {id}"),
        approved: level,
        scopes: scopes.to_vec(),
        bots: None,
        cloud: false,
        folders: Vec::new(),
        plain_threads: true,
        via: None,
    }
}

/// A task of `bot`'s box that `thread` works on, started by `client` at `level` (none: the user started it).
fn put_task(hub: &Hub, bot: &str, id: &str, thread: &str, by: Option<(&str, Level)>, setup: Value) {
    let mut task = json!({"id": id, "title": id, "brief": "x", "status": "working", "order": 1, "createdAt": 1,
        "startedAt": 2, "thread": thread, "source": {"kind": "user"}});
    task["setup"] = setup;
    if let Some((client, level)) = by {
        task["startedBy"] = json!({"client": client, "label": format!("App {client}"), "level": level});
    }
    let key = format!("work.{bot}");
    let mut all: Vec<Value> = hub.store.kv_get(&key).and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default();
    all.push(task);
    hub.store.kv_set(&key, &json!(all).to_string()).unwrap();
    let mut t = bot_cfg(thread);
    t.thread_of = Some(bot.into());
    hub.store.save_bot(&t).unwrap();
}

fn bot_cfg(id: &str) -> BotConfig {
    bot(id, "Mika", "/tmp")
}

fn code(e: &anyhow::Error) -> Code {
    e.downcast_ref::<Refusal>().unwrap_or_else(|| panic!("not a refusal: {e:#}")).code
}

async fn refused(hub: &Arc<Hub>, p: &Principal, tool: &str, args: Value) -> Code {
    code(&call(hub, p, tool, &args).await.expect_err("refused"))
}

#[tokio::test]
async fn an_app_sees_its_own_threads_and_others_only_with_the_scope_never_a_plain_one() {
    let hub = hub();
    let closed = BotConfig { reachable_by_assistants: false, ..bot_cfg("b1") };
    hub.store.save_bot(&closed).unwrap();
    let mut plain = bot_cfg("box");
    plain.plain = true;
    plain.hidden = true;
    hub.store.save_bot(&plain).unwrap();
    put_task(&hub, "b1", "mine", "thread-1", Some(("a", Level::Ask)), json!({}));
    put_task(&hub, "b1", "users", "thread-2", None, json!({}));
    put_task(&hub, "box", "plain-mine", "thread-3", Some(("a", Level::Ask)), json!({"backend": "codex"}));
    put_task(&hub, "box", "plain-users", "thread-4", None, json!({"backend": "codex"}));
    let titles = |v: &Value| -> Vec<String> {
        let mut t: Vec<String> =
            v["threads"].as_array().unwrap().iter().map(|t| t["taskId"].as_str().unwrap().to_owned()).collect();
        t.sort();
        t
    };
    let a = app("a", Level::Ask, &[Scope::Threads]);
    assert_eq!(titles(&call(&hub, &a, "thread_list", &json!({})).await.unwrap()), ["mine", "plain-mine"]);
    let all = app("a", Level::Ask, &[Scope::Threads, Scope::AllThreads]);
    let seen = call(&hub, &all, "thread_list", &json!({})).await.unwrap();
    assert_eq!(titles(&seen), ["mine", "plain-mine", "users"], "never the user's threads without a bot");
    assert_eq!(seen["threads"][0]["startedBy"], "App a");
    let other = app("b", Level::Full, &[Scope::Threads]);
    assert!(titles(&call(&hub, &other, "thread_list", &json!({})).await.unwrap()).is_empty());
    let read = json!({"botId": "box", "taskId": "plain-mine"});
    assert_eq!(refused(&hub, &other, "thread_read", read.clone()).await, Code::NotFound, "another app's");
    assert!(call(&hub, &a, "thread_read", &read).await.is_ok());
    // The cloud assistant reaches only the bots open to cloud assistants.
    let mut cloud = clients::cloud(&hub.store);
    cloud.scopes.push(Scope::AllThreads);
    assert!(titles(&call(&hub, &cloud, "thread_list", &json!({})).await.unwrap()).is_empty());
}

#[tokio::test]
async fn an_apps_thread_runs_at_its_level_after_a_restart_too_and_says_who_started_it() {
    let hub = hub();
    hub.store.save_bot(&bot_cfg("b1")).unwrap();
    put_task(&hub, "b1", "ask", "thread-a", Some(("a", Level::Ask)), json!({}));
    put_task(&hub, "b1", "auto", "thread-b", Some(("a", Level::Auto)), json!({}));
    put_task(&hub, "b1", "full", "thread-c", Some(("a", Level::Full)), json!({}));
    put_task(&hub, "b1", "users", "thread-d", None, json!({}));
    let access = |t: &str| work::access_of(&hub, &hub.store.bot(t).unwrap().unwrap().config);
    assert_eq!(
        (access("thread-a"), access("thread-b"), access("thread-c"), access("thread-d")),
        (Access::Ask, Access::Auto, Access::Allow, Access::Allow)
    );
    let row = &work::wire(&hub, &bot_cfg("b1"))[0];
    assert_eq!(row["startedBy"]["label"], "App a");
    assert!(row.get("source").is_none(), "to an older app it is the user's thread");
    // Its turns are framed as the app's, and memory never takes them as the user's own words.
    let thread = hub.store.bot("thread-a").unwrap().unwrap().config;
    let by = StartedBy { client: "a".into(), label: "App a".into(), level: Level::Ask };
    let mut data = json!({"text": "fix it"});
    by.mark(&mut data, "fix it");
    let e = hub.add_entry(&Lane::main("thread-a"), EntryKind::User, 1, &data).unwrap();
    let prompt = crate::chat::prompt::of_entry(&hub, &thread, &e);
    assert!(prompt.contains("a message from App a, an outside app") && prompt.ends_with("fix it"), "{prompt}");
    assert_eq!(data["pasted"], json!(["fix it"]));
    let told = crate::chat::context::render(&hub.store, &thread);
    assert!(told.contains("started by App a") && told.contains("not the user's own words"), "{told}");
}

#[tokio::test]
async fn a_launch_never_asks_for_more_than_the_app_may() {
    let hub = hub();
    let home = std::env::temp_dir().join(format!("codync-mcp-home-{}", uuid::Uuid::new_v4()));
    let away = std::env::temp_dir().join(format!("codync-mcp-away-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&away).unwrap();
    hub.store.save_bot(&bot("b1", "Mika", home.to_str().unwrap())).unwrap();
    let all = [Scope::Threads];
    let read_only = app("r", Level::ReadOnly, &all);
    let launch = |more: Value| {
        let mut args = json!({"message": "Fix the build", "botId": "b1"});
        args.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
        args
    };
    assert_eq!(refused(&hub, &read_only, "thread_launch", launch(json!({}))).await, Code::CapabilityDenied);
    let auto = app("a", Level::Auto, &all);
    assert_eq!(
        refused(&hub, &auto, "thread_launch", launch(json!({"approval": "full"}))).await,
        Code::ModeEscalationDenied
    );
    let mode = launch(json!({"approval": "auto", "mode": "bypassPermissions"}));
    assert_eq!(refused(&hub, &auto, "thread_launch", mode).await, Code::ModeEscalationDenied);
    let options = launch(json!({"modelOptions": {"permissions": "none"}}));
    assert_eq!(refused(&hub, &auto, "thread_launch", options).await, Code::ModeEscalationDenied);
    // An app approved for Full still acts under the computer's maximum (Auto by default), and under its own limit.
    let full = app("f", Level::Full, &all);
    assert_eq!(
        refused(&hub, &full, "thread_launch", launch(json!({"approval": "full"}))).await,
        Code::ModeEscalationDenied
    );
    clients::set_levels(&hub.store, &json!({"max": "full"})).unwrap();
    let narrowed = launch(json!({"approval": "full", "accessLimit": "ask"}));
    assert_eq!(refused(&hub, &full, "thread_launch", narrowed).await, Code::ModeEscalationDenied);
    // Where it works and on what.
    let outside = launch(json!({"folder": away.to_string_lossy()}));
    assert_eq!(refused(&hub, &auto, "thread_launch", outside).await, Code::CapabilityDenied);
    let no_plain = Principal { plain_threads: false, ..auto.clone() };
    let plain = json!({"message": "x", "agent": "codex"});
    assert_eq!(refused(&hub, &no_plain, "thread_launch", plain).await, Code::CapabilityDenied);
    let elsewhere = Principal { bots: Some(vec!["b2".into()]), ..auto.clone() };
    assert_eq!(refused(&hub, &elsewhere, "thread_launch", launch(json!({}))).await, Code::NotFound);
    // At most so many of an app's threads at once.
    for n in 0..MAX_LIVE {
        put_task(&hub, "b1", &format!("live-{n}"), &format!("thread-{n}"), Some(("a", Level::Ask)), json!({}));
    }
    assert_eq!(refused(&hub, &auto, "thread_launch", launch(json!({}))).await, Code::RateLimited);
}

#[tokio::test]
async fn an_app_acts_only_on_threads_within_its_level_and_never_approves() {
    let hub = hub();
    hub.store.save_bot(&bot_cfg("b1")).unwrap();
    let all = [Scope::Threads, Scope::AllThreads];
    // The user's own thread that approves by itself is Full to an app.
    put_task(&hub, "b1", "free", "thread-f", None, json!({"approval": {"permission": "auto", "mode": null}}));
    put_task(&hub, "b1", "mine", "thread-m", Some(("a", Level::Ask)), json!({}));
    let auto = app("a", Level::Auto, &all);
    let free = json!({"botId": "b1", "taskId": "free"});
    for tool in ["thread_steer", "thread_interrupt", "thread_send"] {
        let mut args = free.clone();
        args["how"] = "stop".into();
        args["message"] = "go on".into();
        assert_eq!(refused(&hub, &auto, tool, args).await, Code::ModeEscalationDenied, "{tool}");
    }
    assert!(call(&hub, &auto, "thread_read", &free).await.is_ok(), "reading is fine");
    // An approval waits: the app sees it, can't answer it.
    let card = hub
        .add_entry(
            &Lane::main("b1"),
            EntryKind::Permission,
            1,
            &json!({"status": "pending", "title": "Run npm test", "thread": {"id": "thread-m"}}),
        )
        .unwrap();
    let mine = json!({"botId": "b1", "taskId": "mine"});
    let listed = call(&hub, &auto, "thread_pending", &mine).await.unwrap();
    assert_eq!(
        (&listed["pending"][0]["kind"], &listed["pending"][0]["summary"]),
        (&json!("permission"), &json!("Run npm test"))
    );
    let answer = json!({"botId": "b1", "taskId": "mine", "action": "answer", "requestId": card.id, "option": "allow"});
    assert_eq!(refused(&hub, &auto, "thread_pending", answer).await, Code::CapabilityDenied);
    let waited =
        call(&hub, &auto, "thread_wait", &json!({"botId": "b1", "taskId": "mine", "timeoutSecs": 0})).await.unwrap();
    assert_eq!(waited["needsYou"][0]["kind"], "permission");
    assert_eq!(waited["timedOut"], true);
}
