use super::*;
use crate::remote::identity::Identity;
use crate::store::{BotConfig, EntryKind, Lane, Store};
use crate::whatsapp::{LinkState, settings};
use std::path::PathBuf;

const OWNER: &str = "owner@s.whatsapp.net";
const SENDER: &str = "sender@s.whatsapp.net";

struct Fixture {
    hub: Arc<Hub>,
    root: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("codync-whatsapp-inbox-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let hub = Hub::new(
            Store::open(std::path::Path::new(":memory:")).unwrap(),
            "test".into(),
            Identity::load_or_create(&root).unwrap(),
            "test".into(),
            0,
        );
        for id in ["a", "b"] {
            let cfg: BotConfig =
                serde_json::from_value(json!({"id":id,"name":id,"backend":"fixture","cwd":root})).unwrap();
            hub.store.save_bot(&cfg).unwrap();
        }
        *hub.whatsapp.state.try_lock().unwrap() =
            LinkState { state: "connected".into(), me: Some(OWNER.into()), ..LinkState::default() };
        settings::record_account(&hub, OWNER).unwrap();
        settings::set_bot(&hub, "a").unwrap();
        settings::set_allowed(&hub, SENDER, true).unwrap();
        Self { hub, root }
    }

    fn event() -> Value {
        json!({"_codyncBotId":"a","_codyncUsesDefault":false,"_codyncMe":OWNER,
            "chat":SENDER,"sender":SENDER,"group":false,"text":"hello","id":"message"})
    }
}

#[test]
fn hermes_whatsapp_notes_keep_their_target_when_other_bots_or_default_bindings_change() {
    let fixture = Fixture::new();
    let hub = &fixture.hub;
    let event = Fixture::event();
    let before = snapshot(hub, &event, OWNER).unwrap().unwrap();
    let mut other = hub.store.bot("b").unwrap().unwrap().config;
    other.name = "Renamed teammate".into();
    hub.store.save_bot(&other).unwrap();
    settings::set_bot(hub, "b").unwrap();
    assert_eq!(snapshot(hub, &event, OWNER).unwrap().as_deref(), Some(before.as_str()));
    let mut default_event = event.clone();
    default_event["_codyncUsesDefault"] = true.into();
    assert!(snapshot(hub, &default_event, OWNER).unwrap().is_none());
    let mut target = hub.store.bot("a").unwrap().unwrap().config;
    target.hidden = true;
    hub.store.save_bot(&target).unwrap();
    assert!(snapshot(hub, &event, OWNER).unwrap().is_none());
}

#[test]
fn hermes_whatsapp_revocation_cannot_be_undone_by_restoring_the_same_allowlist() {
    let fixture = Fixture::new();
    let hub = &fixture.hub;
    let event = Fixture::event();
    let generation = snapshot(hub, &event, OWNER).unwrap().unwrap();
    hub.add_entry_once("input", &Lane::main("a"), EntryKind::User, 1,
        &json!({"text":"hello","status":"queued","source":"whatsapp", "whatsappFence":event,"whatsappGeneration":generation})).unwrap();
    assert!(claim_prompt(hub, "input").unwrap());
    settings::set_allowed(hub, SENDER, false).unwrap();
    assert!(!claim_prompt(hub, "input").unwrap());
    settings::set_allowed(hub, SENDER, true).unwrap();
    assert!(!claim_prompt(hub, "input").unwrap());
    assert_ne!(snapshot(hub, &event, OWNER).unwrap().as_deref(), Some(generation.as_str()));
}

#[tokio::test]
async fn hermes_whatsapp_notes_survive_temporary_disconnect_without_running_a_bot() {
    let fixture = Fixture::new();
    let hub = &fixture.hub;
    let event = Fixture::event();
    let generation = snapshot(hub, &event, OWNER).unwrap().unwrap();
    hub.store
        .kv_set(KEY, &serde_json::to_string(&vec![Pending { id: "input".into(), generation, event }]).unwrap())
        .unwrap();
    hub.whatsapp.state.lock().await.state = "disconnected".into();
    tick(&hub.whatsapp, hub).await.unwrap();
    assert_eq!(read(hub).unwrap().len(), 1);
    invalidate_link(hub).unwrap();
    assert!(read(hub).unwrap().is_empty());
}

#[test]
fn whatsapp_decisions_keep_the_original_author_when_another_participant_writes() {
    let fixture = Fixture::new();
    let hub = &fixture.hub;
    let lane = Lane::in_thread("a", "root");
    let input = |sender| {
        json!({"source":"whatsapp", "whatsappChat":"group@g.us",
        "whatsappFence":{"sender":sender}, "status":"sent"})
    };
    hub.add_entry_once("first", &lane, EntryKind::User, 1, &input("first@s.whatsapp.net")).unwrap();
    hub.add_entry_once("later", &lane, EntryKind::User, 2, &input("later@s.whatsapp.net")).unwrap();
    let (mut card, _) = hub
        .add_entry_once(
            "card",
            &lane,
            EntryKind::Permission,
            1,
            &json!({"inputEntryIds":["first"], "status":"pending"}),
        )
        .unwrap();
    assert_eq!(
        crate::whatsapp::approvals::sender(hub, &card, "group@g.us").unwrap().as_deref(),
        Some("first@s.whatsapp.net")
    );
    card.data["inputEntryIds"] = json!(["first", "later"]);
    assert!(crate::whatsapp::approvals::sender(hub, &card, "group@g.us").unwrap().is_none());
    card.data["inputEntryIds"] = json!([]);
    assert!(crate::whatsapp::approvals::sender(hub, &card, "group@g.us").unwrap().is_none());
}

#[tokio::test]
async fn accepted_whatsapp_input_survives_socket_close_but_not_transport_replacement() {
    let fixture = Fixture::new();
    let hub = &fixture.hub;
    let mut event = Fixture::event();
    event["_codyncTransportEpoch"] = 0.into();
    event["_codyncTransportMe"] = OWNER.into();
    hub.whatsapp.state.lock().await.state = "disconnected".into();
    receive(&hub.whatsapp, hub, &event).await.unwrap();
    assert_eq!(read(hub).unwrap().len(), 1, "save already accepted input even after the socket closes");
    hub.whatsapp.transport_epoch.fetch_add(1, Ordering::SeqCst);
    event["id"] = "from-old-reader".into();
    receive(&hub.whatsapp, hub, &event).await.unwrap();
    assert_eq!(read(hub).unwrap().len(), 1, "an old reader cannot inject input into its replacement");
}

#[tokio::test]
async fn old_whatsapp_sends_cannot_cross_a_link_replacement_while_waiting_to_write() {
    for rotate_epoch in [false, true] {
        let fixture = Fixture::new();
        let app = &fixture.hub.whatsapp;
        let lifecycle = app.lifecycle.lock().await;
        let mut sending = std::pin::pin!(app.send_message(SENDER, "private old-account reply", None, None, None));
        // Poll through the initial identity snapshot to the held lifecycle lock, without
        // starting a subprocess, writing a pipe or relying on timing/sleeps.
        tokio::select! {
            biased;
            result = &mut sending => panic!("send passed a held lifecycle lock: {result:?}"),
            () = async {} => {}
        }
        if rotate_epoch {
            app.transport_epoch.fetch_add(1, Ordering::SeqCst);
        } else {
            app.state.lock().await.me = Some("replacement@s.whatsapp.net".into());
        }
        drop(lifecycle);
        let error = sending.await.unwrap_err();
        assert!(
            error.to_string().contains("connection changed"),
            "must refuse before attempting bridge I/O: {error:#}"
        );
    }
}

#[test]
fn queued_whatsapp_reports_keep_their_original_link_generation_on_replay() {
    let fixture = Fixture::new();
    let hub = &fixture.hub;
    let output = crate::routines::Output {
        kind: "whatsapp".into(),
        target: SENDER.into(),
        account: Some(OWNER.into()),
        value: None,
    };
    let intent = hub.store.snapshot_whatsapp_output(&json!({"source":"test"})).unwrap();
    assert_eq!(intent["source"], "test", "pinning must preserve explicit tests of paused routines");
    hub.store.queue_routine_outputs("run", "routine", std::slice::from_ref(&output), &intent, "report").unwrap();
    let job = hub.store.pending_routine_outputs().unwrap().remove(0);
    assert!(crate::whatsapp::delivery::authorized(hub, &output, &job.event).unwrap());
    invalidate_link(hub).unwrap();
    assert!(!crate::whatsapp::delivery::authorized(hub, &output, &job.event).unwrap());
    settings::record_account(hub, OWNER).unwrap();
    let new_intent = hub.store.snapshot_whatsapp_output(&json!({"source":"test"})).unwrap();
    hub.store
        .queue_routine_outputs("run", "routine", std::slice::from_ref(&output), &new_intent, "report replay")
        .unwrap();
    let replay = hub.store.pending_routine_outputs().unwrap().remove(0);
    assert_eq!(job.event, replay.event, "replaying the same run must not acquire new authorization");
    assert!(!crate::whatsapp::delivery::authorized(hub, &replay.output, &replay.event).unwrap());
    hub.store
        .queue_routine_outputs("late-flow", "routine", &[output], &intent, "old work completed after relink")
        .unwrap();
    let late = hub.store.pending_routine_outputs().unwrap().into_iter().find(|job| job.run == "late-flow").unwrap();
    assert!(
        !crate::whatsapp::delivery::authorized(hub, &late.output, &late.event).unwrap(),
        "a first enqueue after relinking must retain the durable intent's original authorization"
    );
}

#[tokio::test]
async fn whatsapp_reports_distinguish_unsent_offline_from_revoked_authorization() {
    let fixture = Fixture::new();
    let hub = &fixture.hub;
    let event = hub.store.snapshot_whatsapp_output(&json!({})).unwrap();
    hub.whatsapp.state.lock().await.state = "disconnected".into();
    let error =
        hub.whatsapp.send_bound_text(&hub.store, OWNER, &event["whatsappBinding"], SENDER, "report").await.unwrap_err();
    assert!(
        error.is::<crate::whatsapp::delivery::Unsent>(),
        "disconnection before any bridge I/O must defer the report"
    );
    invalidate_link(hub).unwrap();
    let error =
        hub.whatsapp.send_bound_text(&hub.store, OWNER, &event["whatsappBinding"], SENDER, "report").await.unwrap_err();
    assert!(
        !error.is::<crate::whatsapp::delivery::Unsent>(),
        "revocation must never become permission to retry on a new link"
    );
}

#[tokio::test]
async fn replacing_a_whatsapp_link_preserves_history_but_revokes_old_routes_cards_and_loaded_batches() {
    for logout in [false, true] {
        let fixture = Fixture::new();
        let hub = &fixture.hub;
        hub.store.map_whatsapp_thread(SENDER, "a", "old-root", SENDER).unwrap();
        hub.add_entry_once(
            "old-reply",
            &Lane::in_thread("a", "old-root"),
            EntryKind::Agent,
            1,
            &json!({"text":"private first-account reply","final":true}),
        )
        .unwrap();
        hub.add_entry_once(
            "old-card",
            &Lane::in_thread("a", "old-root"),
            EntryKind::Permission,
            1,
            &json!({"status":"pending"}),
        )
        .unwrap();
        hub.store.mark_whatsapp_card(SENDER, "old-card", SENDER).unwrap();
        let old_binding = hub.store.snapshot_whatsapp_output(&json!({})).unwrap()["whatsappBinding"].clone();
        let loaded = hub.store.pending_whatsapp_replies().unwrap();
        assert_eq!(loaded.len(), 1);
        if logout {
            invalidate_link(hub).unwrap();
        }
        let replacement = "replacement@s.whatsapp.net";
        settings::record_account(hub, replacement).unwrap();
        hub.whatsapp.state.lock().await.me = Some(replacement.into());
        assert!(hub.store.whatsapp_root(SENDER, "a").unwrap().is_none());
        assert!(hub.store.whatsapp_card(SENDER, SENDER).is_none());
        assert!(hub.store.pending_whatsapp_replies().unwrap().is_empty());
        assert!(hub.store.entry_checked("old-reply").unwrap().is_some(), "the chat history is preserved");
        let error = hub
            .whatsapp
            .send_bound_text(&hub.store, OWNER, &old_binding, &loaded[0].chat, &loaded[0].text)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("revoked"), "a loaded batch must fail before bridge I/O");
        let error = hub
            .whatsapp
            .send_bound_file(&hub.store, &old_binding, SENDER, &json!({"path":"never-opened","name":"private.txt"}))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("revoked"), "attachments use the same original account binding");
    }
}

#[tokio::test]
async fn provider_logout_revokes_the_link_even_without_pressing_disconnect_in_codync() {
    let fixture = Fixture::new();
    let hub = &fixture.hub;
    hub.store.map_whatsapp_thread(SENDER, "a", "old-root", SENDER).unwrap();
    let binding = hub.store.snapshot_whatsapp_output(&json!({})).unwrap()["whatsappBinding"].clone();
    hub.whatsapp.on_event(hub, json!({"type":"state","state":"loggedOut"})).await;
    assert!(!crate::whatsapp::delivery::matches(&hub.store, OWNER, &binding).unwrap());
    assert!(hub.store.whatsapp_root(SENDER, "a").unwrap().is_none());
    assert_eq!(hub.whatsapp.state().await.state, "loggedOut");
    assert!(!hub.whatsapp.revocation_pending.load(Ordering::SeqCst));
}
