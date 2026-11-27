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
        session: None,
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
    // So is one with no approval of its own on a bot that approves by itself.
    let free_bot = BotConfig { permission: crate::store::Permission::Auto, ..bot_cfg("b2") };
    hub.store.save_bot(&free_bot).unwrap();
    put_task(&hub, "b2", "inherits", "thread-i", None, json!({}));
    let inherits = json!({"botId": "b2", "taskId": "inherits", "how": "stop"});
    assert_eq!(refused(&hub, &auto, "thread_steer", inherits).await, Code::ModeEscalationDenied);
    // Only answering writes: any other action reads, and one it doesn't know is refused.
    let odd = json!({"botId": "b1", "taskId": "mine", "action": "Answer"});
    assert_eq!(refused(&hub, &auto, "thread_pending", odd).await, Code::InvalidRequest);
    let reader = app("r", Level::ReadOnly, &all);
    let answer = json!({"botId": "b1", "taskId": "mine", "action": "answer", "requestId": "x", "option": "A"});
    assert_eq!(refused(&hub, &reader, "thread_pending", answer).await, Code::CapabilityDenied);
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

/// An ended (`done`, at `ended`) or waiting task of `bot`'s, started by `client` (none: the user).
fn put_row(hub: &Hub, bot: &str, id: &str, client: Option<&str>, ended: Option<i64>) {
    let status = if ended.is_some() { "done" } else { "waiting" };
    let mut task = json!({"id": id, "title": id, "brief": "x", "status": status, "order": 1, "createdAt": 1,
        "startedAt": 1, "endedAt": ended, "source": {"kind": "user"}});
    if let Some(client) = client {
        task["startedBy"] = json!({"client": client, "label": format!("App {client}"), "level": "ask"});
    }
    let key = format!("work.{bot}");
    let mut all: Vec<Value> = hub.store.kv_get(&key).and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default();
    all.push(task);
    hub.store.kv_set(&key, &json!(all).to_string()).unwrap();
}

/// Every page of `thread_list` for `p` from the first, its task ids in order.
async fn every_page(hub: &Arc<Hub>, p: &Principal, args: Value) -> Vec<String> {
    let mut ids = Vec::new();
    let mut args = args;
    loop {
        let page = call(hub, p, "thread_list", &args).await.unwrap();
        ids.extend(page["threads"].as_array().unwrap().iter().map(|t| t["taskId"].as_str().unwrap().to_owned()));
        let Some(next) = page["next"].as_str() else { return ids };
        args["cursor"] = next.into();
    }
}

#[tokio::test]
async fn thread_list_pages_through_every_thread_the_app_sees_live_ones_first() {
    const DAY: i64 = 24 * 60 * 60 * 1000;
    let hub = hub();
    let now = crate::store::now_ms();
    for id in ["b1", "b2"] {
        hub.store.save_bot(&bot_cfg(id)).unwrap();
    }
    // b1: 20 of the app's threads ended within the day; b2: 3 of its threads that wait.
    for n in 0..20 {
        put_row(&hub, "b1", &format!("recent-{n}"), Some("a"), Some(now - 1000 - n));
    }
    for n in 0..3 {
        put_row(&hub, "b2", &format!("live-{n}"), Some("a"), None);
    }
    // Older than a day: 35 of the user's, then 5 of the app's behind them.
    for n in 0..35 {
        put_row(&hub, "b1", &format!("users-{n}"), None, Some(now - 2 * DAY - n));
    }
    for n in 0..5 {
        put_row(&hub, "b1", &format!("old-{n}"), Some("a"), Some(now - 3 * DAY - n));
    }
    let a = app("a", Level::Ask, &[Scope::Threads]);
    let first = call(&hub, &a, "thread_list", &json!({"limit": 20})).await.unwrap();
    let shown: Vec<&str> = first["threads"].as_array().unwrap().iter().map(|t| t["taskId"].as_str().unwrap()).collect();
    assert_eq!(shown.len(), 20);
    assert!(shown[..3].iter().all(|t| t.starts_with("live-")), "live ones first, never cut: {shown:?}");
    let ids = every_page(&hub, &a, json!({"limit": 20})).await;
    let mut unique = ids.clone();
    unique.sort();
    unique.dedup();
    assert_eq!((ids.len(), unique.len()), (28, 28), "each once, the user's never: {ids:?}");
    assert!(ids.iter().filter(|t| t.starts_with("old-")).count() == 5, "past the pages of the user's: {ids:?}");
    // One box, a page larger than the archive's, and a page of live ones only: still every one.
    assert_eq!(every_page(&hub, &a, json!({"limit": 50, "botId": "b1"})).await.len(), 25);
    let live_only = call(&hub, &a, "thread_list", &json!({"limit": 2})).await.unwrap();
    assert_eq!(live_only["threads"].as_array().unwrap().len(), 3);
    assert!(live_only["next"].is_string(), "it leads on to the ended ones");
}

#[tokio::test]
async fn an_app_never_ends_or_reruns_the_owners_work_nor_goes_past_its_live_threads() {
    let hub = hub();
    clients::set_levels(&hub.store, &json!({"max": "full"})).unwrap();
    hub.store.save_bot(&bot_cfg("b1")).unwrap();
    let full = app("a", Level::Full, &[Scope::Threads, Scope::AllThreads]);
    // A routine's run that stopped.
    let mut run = json!({"id": "run", "title": "run", "brief": "x", "status": "stopped", "order": 1, "createdAt": 1,
        "endedAt": 5, "source": {"kind": "routine", "id": "r", "run": "x"}});
    run["setup"] = json!({});
    hub.store.kv_set("work.b1", &json!([run]).to_string()).unwrap();
    for how in ["retry", "stop", "drop"] {
        let args = json!({"botId": "b1", "taskId": "run", "how": how});
        assert_eq!(refused(&hub, &full, "thread_steer", args).await, Code::CapabilityDenied, "{how}");
    }
    let now = json!({"botId": "b1", "taskId": "run", "how": "now"});
    assert_eq!(refused(&hub, &full, "thread_steer", now).await, Code::InvalidRequest, "no other work gives way");
    // Its own thread that ended, while as many of its threads as it may have are going: neither a retry nor a
    // message reopens it.
    put_task(&hub, "b1", "ended", "thread-e", Some(("a", Level::Ask)), json!({}));
    let mut all: Vec<Value> = serde_json::from_str(&hub.store.kv_get("work.b1").unwrap()).unwrap();
    let task = all.iter_mut().find(|t| t["id"] == "ended").unwrap();
    (task["status"], task["endedAt"]) = (json!("stopped"), json!(5));
    hub.store.kv_set("work.b1", &json!(all).to_string()).unwrap();
    for n in 0..MAX_LIVE {
        put_task(&hub, "b1", &format!("live-{n}"), &format!("thread-{n}"), Some(("a", Level::Ask)), json!({}));
    }
    let retry = json!({"botId": "b1", "taskId": "ended", "how": "retry"});
    assert_eq!(refused(&hub, &full, "thread_steer", retry).await, Code::RateLimited);
    let send = json!({"botId": "b1", "taskId": "ended", "message": "again"});
    assert_eq!(refused(&hub, &full, "thread_send", send).await, Code::RateLimited);
}
