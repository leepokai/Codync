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
    }
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
    assert!(routines.enqueue(&store, "bot", "routine", Value::Null, None, false).is_err());
}
#[test]
fn a_delivery_while_a_retry_waits_is_queued() {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let routines = Routines::default();
    let r = routine();
    let mut waiting = new_run(&r, json!({"text": "first"}), None);
    waiting.not_before = Some(now_ms() + 60_000);
    routines
        .change(&store, |s| {
            s.routines.push(r);
            s.runs.push(waiting);
            Ok(())
        })
        .unwrap();
    routines.enqueue(&store, "bot", "routine", json!({"text": "second"}), None, false).unwrap();
    let third = routines.enqueue(&store, "bot", "routine", json!({"text": "third"}), None, false);
    assert_eq!(third.unwrap_err().downcast::<Refused>().unwrap(), Refused::Busy, "the queued delivery is");
    assert_eq!(routines.state.locked().runs.len(), 2);
}
