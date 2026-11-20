use super::*;
pub(super) fn routine() -> Routine {
    Routine {
        id: "routine".into(),
        bot_id: "bot".into(),
        name: "Check".into(),
        instruction: "Check status".into(),
        triggers: vec![Trigger::Interval { seconds: 60 }],
        enabled: true,
        created_at: 0,
        updated_at: 0,
        next_runs: vec![Some(60_000)],
        webhook_key: "secret".into(),
        deleted: false,
        timeout_seconds: default_timeout(),
        last_error: None,
        progress: None,
    }
}
/// An in-memory store and `Routines` holding `routine()`.
pub(super) fn seeded() -> (Store, Routines) {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let routines = Routines::default();
    routines
        .change(&store, |s| {
            s.routines.push(routine());
            Ok(())
        })
        .unwrap();
    (store, routines)
}
#[test]
fn pending_runs_survive_restart_but_running_runs_are_not_duplicated() {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let routines = Routines::default();
    let r = routine();
    let pending = new_run(&r, Value::Null, None);
    let mut running = new_run(&r, Value::Null, None);
    running.status = Status::Running;
    routines
        .change(&store, |s| {
            s.routines.push(r);
            s.runs.extend([pending, running]);
            Ok(())
        })
        .unwrap();
    let restored = Routines::default();
    restored.restore(&store).unwrap();
    let state = restored.state.locked();
    assert!(state.runs[0].status == Status::Pending);
    assert!(state.runs[1].status == Status::Recovering);
}
#[test]
fn pause_cancels_pending_and_resume_recalculates_schedule() {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let routines = Routines::default();
    let r = routine();
    routines
        .change(&store, |s| {
            s.runs.push(new_run(&r, Value::Null, None));
            s.routines.push(r);
            Ok(())
        })
        .unwrap();
    routines.set_enabled(&store, "bot", "routine", false).unwrap();
    assert!(routines.state.locked().runs[0].status == Status::Cancelled);
    routines.set_enabled(&store, "bot", "routine", true).unwrap();
    assert!(routines.state.locked().routines[0].next_runs[0].unwrap() > now_ms());
}
#[test]
fn duplicate_delivery_is_idempotent_and_cross_bot_edits_fail() {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let routines = Routines::default();
    routines
        .change(&store, |s| {
            s.routines.push(routine());
            Ok(())
        })
        .unwrap();
    let first = routines.enqueue(&store, "bot", "routine", Value::Null, Some("same".into()), false).unwrap();
    let second = routines.enqueue(&store, "bot", "routine", Value::Null, Some("same".into()), false).unwrap();
    assert_eq!(first["run"]["id"], second["run"]["id"]);
    assert!(routines.set_enabled(&store, "other", "routine", false).is_err());
    assert!(routines.enqueue(&store, "other", "routine", Value::Null, None, true).is_err());
    assert!(routines.owns("bot", "routine") && !routines.owns("other", "routine"));
    assert!(routines.enqueue(&store, "bot", "routine", Value::Null, None, false).is_ok(), "a new one queues behind");
}
#[test]
fn a_delivery_while_a_retry_waits_is_queued() {
    let (store, routines) = seeded();
    let mut waiting = new_run(&routine(), json!({"text": "first"}), None);
    waiting.not_before = Some(now_ms() + 60_000);
    routines.state.locked().runs.push(waiting);
    routines.enqueue(&store, "bot", "routine", json!({"text": "second"}), None, false).unwrap();
    routines.enqueue(&store, "bot", "routine", json!({"text": "third"}), None, false).unwrap();
    assert_eq!(routines.state.locked().runs.len(), 3, "queued runs don't block a delivery");
}

#[test]
fn a_delivery_while_the_first_waits_for_its_busy_bot_is_queued() {
    // Issue #502: the first delivery is queued (Pending, waiting for the busy bot to be free);
    // a second delivery for the same routine must be queued too, not refused as busy.
    let (store, routines) = seeded();
    routines.enqueue(&store, "bot", "routine", json!({"text": "first"}), Some("d1".into()), false).unwrap();
    assert!(routines.state.locked().runs[0].status == Status::Pending, "the first waits for its bot");
    let second = routines.enqueue(&store, "bot", "routine", json!({"text": "second"}), Some("d2".into()), false);
    assert!(second.is_ok(), "the second delivery is refused: {:?}", second.err().map(|e| e.to_string()));
    let runs: Vec<_> = routines.state.locked().runs.iter().map(|r| r.delivery_id.clone().unwrap()).collect();
    assert_eq!(runs, ["d1", "d2"], "each once, in arrival order");
}

#[test]
fn a_delivery_waits_while_a_run_has_started_or_the_queue_is_full() {
    let (store, routines) = seeded();
    let deliver = |n: usize| routines.enqueue(&store, "bot", "routine", json!({"n": n}), Some(format!("d{n}")), false);
    for n in 0..MAX_QUEUED {
        deliver(n).unwrap();
    }
    let refusal = |e: anyhow::Error| e.downcast::<Refused>().unwrap();
    assert_eq!(refusal(deliver(99).unwrap_err()), Refused::Busy, "the queue is full");
    assert!(deliver(0).is_ok(), "a retried delivery still finds its run");
    routines.state.locked().runs.truncate(1);
    routines.state.locked().runs[0].status = Status::Running;
    assert_eq!(refusal(deliver(99).unwrap_err()), Refused::Busy, "a run has started");
}

#[test]
fn a_queued_run_waits_for_its_routines_started_run_and_the_next_starts_in_order() {
    let (store, routines) = seeded();
    for n in 1..=2 {
        routines.enqueue(&store, "bot", "routine", json!({"n": n}), Some(format!("d{n}")), false).unwrap();
    }
    let ids: Vec<_> = routines.state.locked().runs.iter().map(|r| r.id.clone()).collect();
    assert!(!routines.waits_its_turn(&ids[0]) && !routines.waits_its_turn(&ids[1]), "none has started");
    routines.state.locked().runs[0].status = Status::Running;
    assert!(routines.waits_its_turn(&ids[1]), "a second thread doesn't take d2 while d1 runs");
    routines.state.locked().runs[0].status = Status::Succeeded;
    assert!(!routines.waits_its_turn(&ids[1]), "d2 goes once d1 ended");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_routine_run_cannot_add_task_to_the_bots_box() {
    let dir = std::env::temp_dir().join(format!("codync-routine-work-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(&dir.join("t.db")).unwrap();
    store.save_bot(&serde_json::from_value(json!({"id": "bot", "name": "B", "backend": "claude"})).unwrap()).unwrap();
    let identity = crate::remote::identity::Identity::load_or_create(&dir).unwrap();
    let hub = Hub::new(store, "test".into(), identity, "test".into(), 0);
    let add = || crate::chat::work::call(&hub, "bot", "add_task", &json!({"title": "Dig", "brief": "Look into it"}));
    let refusal = "add_task works only in your own chat with the user: its report comes back there";
    assert_eq!(add().unwrap_err().to_string(), refusal, "no turn runs in its chat");

    // A turn runs in its own chat (so nothing starts before it ends).
    hub.set_runtime("bot", |r| {
        r.status = crate::hub::BotStatus::Working;
        r.lane = Some(Lane::main("bot"));
    });
    assert!(!hub.routines.running("bot"));
    assert_eq!(add().unwrap()["status"], "waiting");

    // The same lane, but the turn is a run's: its reply is the run's result, not a message to the user.
    let r = routine();
    let mut run = new_run(&r, Value::Null, None);
    run.status = Status::Running;
    let started = hub.routines.change(&hub.store, |s| {
        s.runs.push(run);
        Ok(())
    });
    started.unwrap();
    assert!(hub.routines.running("bot"));
    assert_eq!(add().unwrap_err().to_string(), refusal);
    let _ = std::fs::remove_dir_all(dir);
}
