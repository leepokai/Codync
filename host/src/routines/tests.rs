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
fn after(id: &str, upstream: &str, event: &str, filters: &[(&str, &str)]) -> Routine {
    let mut all: std::collections::BTreeMap<String, String> =
        filters.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
    all.insert("routineId".into(), upstream.into());
    Routine {
        id: id.into(),
        name: format!("After {upstream}"),
        triggers: vec![Trigger::Event { source: "routine".into(), event: event.into(), filters: all }],
        next_runs: vec![None],
        ..routine()
    }
}
/// Run `i` of `s` ends with `status` and `reply`, as `complete` records it.
fn finish(s: &mut State, i: usize, status: Status, reply: Option<&str>) {
    s.runs[i].status = status;
    s.runs[i].finished_at = Some(now_ms());
    chain::emit(s, i, reply);
}
fn queued(s: &State, routine: &str) -> Vec<Value> {
    s.runs.iter().filter(|r| r.routine_id == routine && r.status == Status::Pending).map(|r| r.event.clone()).collect()
}
#[test]
fn a_finished_run_queues_the_routines_after_it_with_its_result() {
    let mut s = State { routines: vec![routine(), after("next", "routine", "succeeded", &[])], runs: vec![] };
    s.runs.push(new_run(&s.routines[0], json!({"source":"schedule"}), None));
    finish(&mut s, 0, Status::Succeeded, Some("CRIT: disk full"));
    let events = queued(&s, "next");
    assert_eq!(events.len(), 1);
    assert_eq!((events[0]["event"].as_str(), events[0]["text"].as_str()), (Some("succeeded"), Some("CRIT: disk full")));
    assert_eq!((events[0]["passed"].as_str(), events[0]["depth"].as_u64()), (Some("false"), Some(1)));
}
#[test]
fn filters_pick_the_outcome_and_the_result() {
    let mut s = State {
        routines: vec![
            routine(),
            after("crit", "routine", "*", &[("messageContains", "CRIT")]),
            after("reported", "routine", "succeeded", &[("passed", "false")]),
            after("failed", "routine", "failed", &[]),
        ],
        runs: vec![],
    };
    s.runs.push(new_run(&s.routines[0], Value::Null, None));
    finish(&mut s, 0, Status::Succeeded, Some("(pass)"));
    assert!(s.runs.len() == 1, "a pass has nothing to report and no CRIT");
    s.runs.push(new_run(&s.routines[0], Value::Null, None));
    finish(&mut s, 1, Status::Succeeded, Some("WARN only"));
    assert_eq!((queued(&s, "crit").len(), queued(&s, "reported").len(), queued(&s, "failed").len()), (0, 1, 0));
}
#[test]
fn only_a_final_failure_counts_and_a_cancelled_run_triggers_nothing() {
    let mut s = State { routines: vec![routine(), after("next", "routine", "failed", &[])], runs: vec![] };
    s.runs.push(new_run(&s.routines[0], Value::Null, None));
    s.runs[0].retry_at = Some(now_ms() + 60_000);
    finish(&mut s, 0, Status::Failed, None);
    s.runs.push(new_run(&s.routines[0], Value::Null, None));
    finish(&mut s, 1, Status::Cancelled, None);
    assert_eq!(queued(&s, "next"), Vec::<Value>::new());
    s.runs.push(new_run(&s.routines[0], Value::Null, None));
    s.runs[2].detail = Some("agent exited with status 1".into());
    finish(&mut s, 2, Status::Failed, None);
    let event = &queued(&s, "next")[0];
    assert_eq!((event["passed"].as_str(), event["text"].as_str()), (Some("false"), Some("agent exited with status 1")));
    assert_eq!(event["error"], "agent exited with status 1");
}
#[test]
fn any_outcome_with_something_to_report_matches_a_failure() {
    let mut s =
        State { routines: vec![routine(), after("next", "routine", "*", &[("passed", "false")])], runs: vec![] };
    s.runs.push(new_run(&s.routines[0], Value::Null, None));
    finish(&mut s, 0, Status::Failed, None);
    s.runs.push(new_run(&s.routines[0], Value::Null, None));
    finish(&mut s, 2, Status::Succeeded, Some("(pass)"));
    let events = queued(&s, "next");
    assert_eq!((events.len(), events[0]["event"].as_str()), (1, Some("failed")));
    assert_eq!(events[0]["text"], "The run did not finish");
}
#[test]
fn a_busy_or_paused_routine_is_skipped_and_a_long_chain_stops() {
    let mut paused = after("paused", "routine", "succeeded", &[]);
    paused.enabled = false;
    let mut s = State { routines: vec![routine(), after("next", "routine", "succeeded", &[]), paused], runs: vec![] };
    s.runs.push(new_run(&s.routines[1], Value::Null, None));
    s.runs.push(new_run(&s.routines[0], Value::Null, None));
    finish(&mut s, 1, Status::Succeeded, Some("done"));
    assert_eq!(s.runs.len(), 2, "next is already queued, paused doesn't run");
    assert!(s.routines[1].last_error.as_deref().is_some_and(|e| e.contains("still running")));
    // The fourth routine in a row queues the fifth; the fifth queues nothing.
    s.runs.clear();
    s.runs.push(new_run(&s.routines[0], json!({"source":"routine","depth":3}), None));
    finish(&mut s, 0, Status::Succeeded, Some("done"));
    assert_eq!(queued(&s, "next").len(), 1);
    s.runs.clear();
    s.runs.push(new_run(&s.routines[0], json!({"source":"routine","depth":4}), None));
    finish(&mut s, 0, Status::Succeeded, Some("done"));
    assert_eq!(queued(&s, "next"), Vec::<Value>::new());
    assert!(s.routines[1].last_error.as_deref().is_some_and(|e| e.contains("at most 5")));
}
#[test]
fn saving_rejects_an_unknown_upstream_and_a_loop() {
    let s = State { routines: vec![routine(), after("b", "routine", "succeeded", &[])], runs: vec![] };
    let triggers = |up: &str| after("x", up, "succeeded", &[]).triggers;
    assert!(chain::check_links(&s, None, &triggers("missing"), &[]).is_err());
    assert!(chain::check_links(&s, None, &triggers("b"), &[]).is_ok());
    assert!(chain::check_links(&s, Some("routine"), &triggers("b"), &[]).is_err(), "routine → b → routine");
    assert!(chain::check_links(&s, Some("routine"), &triggers("routine"), &[]).is_err());
    let kept = triggers("missing");
    assert!(chain::check_links(&s, Some("x"), &kept, &kept).is_ok(), "a link the routine already had");
    assert!(after("x", "b", "started", &[]).triggers[0].validate(0).is_err());
    assert!(after("x", "b", "*", &[("repo", "a")]).triggers[0].validate(0).is_err());
}
#[test]
fn routine_triggers_need_no_webhook_and_read_with_the_upstream_name() {
    let s = State {
        routines: vec![routine(), after("next", "routine", "succeeded", &[("passed", "false")])],
        runs: vec![],
    };
    assert_eq!(hook_set(&s).iter().map(|h| h.id.as_str()).collect::<Vec<_>>(), Vec::<&str>::new());
    assert_eq!(
        public(&s, &s.routines[1])["triggerDescriptions"][0],
        "After \"Check\" succeeds with something to report"
    );
}
#[test]
fn another_bots_routines_list_without_instructions_or_history() {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let routines = Routines::default();
    let r = routine();
    let run = new_run(&r, Value::Null, None);
    routines
        .change(&store, |s| {
            s.routines.push(r);
            s.runs.push(run);
            Ok(())
        })
        .unwrap();
    let listed = routines.list_followable("bot");
    assert_eq!(
        (listed["routines"][0]["id"].as_str(), listed["routines"][0]["name"].as_str()),
        (Some("routine"), Some("Check"))
    );
    assert!(listed["routines"][0]["instruction"].is_null() && listed["runs"].is_null());
    assert_eq!(routines.list_followable("other")["routines"], json!([]));
}
