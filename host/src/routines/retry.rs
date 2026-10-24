//! Trying failed runs again. A run that failed before its agent used a tool that may change
//! something (a rate limit, an agent that didn't start, a session it couldn't reload) has done
//! nothing yet, so it runs again 1, 5 and 15 minutes later. One that acted may have done part of
//! its work (sent the mail, then failed), and one that hit its time limit could hold the bot that
//! long again: only the user retries those, from the failure notice.
use super::{
    Context, Hub, Refused, Result, Routines, Run, State, Status, Value, bail, busy, json, may_run, new_run, notice_id,
    result_notice_id,
};

/// Seconds before each automatic retry; a run that fails after the last one waits for the user.
const DELAYS: [i64; 3] = [60, 300, 900];

/// Whether run `id` used a tool that may have changed something. A run that never began didn't;
/// one whose check fails counts as acted.
pub fn acted(hub: &Hub, id: &str) -> bool {
    let Some(notice) = hub.store.entry(&notice_id(id)) else { return false };
    hub.store.acted_in_turn(&notice.bot_id, notice.turn).unwrap_or_else(|error| {
        tracing::warn!(run = id, error = format!("{error:#}"), "couldn't check what a failed run did; not retrying it");
        true
    })
}

/// What follows the run at `failed`, which just failed: a retry queued for later when running it
/// again is `safe`, or the run kept for the user to retry. Returns the earlier failures that no
/// longer offer a retry.
pub fn after_failure(s: &mut State, failed: usize, safe: bool, now: i64) -> Vec<String> {
    let run = &s.runs[failed];
    let Some(routine) = s.routines.iter().find(|r| r.id == run.routine_id && !r.deleted) else {
        return Vec::new();
    };
    if let Some(delay) = DELAYS.get(run.attempt as usize).filter(|_| safe && may_run(routine, &run.event)) {
        let attempt = run.attempt + 1;
        // A scheduled fire already queued does the same work: it is the retry (as in `schedule_due`).
        let scheduled = run.event["source"] == "schedule";
        let wake = s.runs.iter().position(|r| {
            scheduled && r.routine_id == routine.id && r.status == Status::Pending && r.event == run.event
        });
        let at = if let Some(wake) = wake {
            s.runs[wake].attempt = attempt;
            s.runs[wake].not_before.unwrap_or(now)
        } else {
            let mut next = new_run(routine, run.event.clone(), None);
            next.attempt = attempt;
            next.not_before = Some(now + delay * 1000);
            s.runs.push(next);
            now + delay * 1000
        };
        s.runs[failed].retry_at = Some(at);
        return Vec::new();
    }
    // Only a routine's latest failure keeps its event, so failed webhook payloads can't pile up.
    let routine_id = routine.id.clone();
    let withdrawn = forget(s, |r| r.routine_id == routine_id);
    s.runs[failed].retryable = true;
    withdrawn
}

/// Takes the retry off the failed runs `matches` picks, dropping the events they kept; returns their ids.
pub fn forget(s: &mut State, matches: impl Fn(&Run) -> bool) -> Vec<String> {
    let mut forgotten = Vec::new();
    for r in s.runs.iter_mut().filter(|r| r.retryable && matches(r)) {
        r.retryable = false;
        r.event = Value::Null;
        forgotten.push(r.id.clone());
    }
    forgotten
}

/// The note on a failure that is tried again automatically, `wait_ms` after it failed.
pub fn retry_note(wait_ms: i64) -> String {
    match (wait_ms + 30_000) / 60_000 {
        ..=0 => "Trying again now.".into(),
        1 => "Trying again in 1 minute.".into(),
        minutes => format!("Trying again in {minutes} minutes."),
    }
}

/// Takes the retry button off these runs' failure notices.
pub fn withdraw(hub: &Hub, runs: &[String]) {
    for id in runs {
        let notice = result_notice_id(id);
        if let Some(mut e) = hub.store.entry(&notice).filter(|e| e.data["retry"] == true) {
            e.data["retry"] = false.into();
            hub.set_entry(&notice, &e.data);
        }
    }
}

impl Routines {
    /// `retryRoutineRun`: does a failed run's work again, with the event it had.
    pub fn retry(&self, hub: &Hub, bot: &str, run_id: &str) -> Result<Value> {
        let run = self.change(&hub.store, |s| {
            let old = s
                .runs
                .iter()
                .position(|r| r.id == run_id && r.bot_id == bot && r.retryable)
                .context("this run can't be retried anymore")?;
            let routine = s
                .routines
                .iter()
                .find(|r| r.id == s.runs[old].routine_id && !r.deleted)
                .context("routine not found")?;
            if !may_run(routine, &s.runs[old].event) {
                bail!(Refused::Paused);
            }
            if busy(s, &routine.id) {
                bail!(Refused::Busy);
            }
            let run = new_run(routine, std::mem::take(&mut s.runs[old].event), None);
            s.runs[old].retryable = false;
            s.runs.push(run.clone());
            Ok(run)
        })?;
        withdraw(hub, &[run_id.to_owned()]);
        Ok(json!({"run": run}))
    }
}

#[cfg(test)]
mod tests {
    use super::super::{EntryKind, Lane, LockExt, Store, tests::routine};
    use super::*;
    use crate::remote::identity::Identity;

    fn failed(attempt: u32) -> (State, usize) {
        let r = routine();
        let mut run: Run = new_run(&r, json!({"source":"schedule"}), None);
        run.status = Status::Failed;
        run.attempt = attempt;
        (State { routines: vec![r], runs: vec![run] }, 0)
    }

    #[test]
    fn a_run_that_did_nothing_is_retried_later_with_its_event() {
        let (mut s, i) = failed(0);
        assert_eq!(after_failure(&mut s, i, true, 1_000), Vec::<String>::new());
        assert_eq!(s.runs[0].retry_at, Some(61_000));
        let next = &s.runs[1];
        assert!(next.status == Status::Pending);
        assert_eq!((next.attempt, next.not_before), (1, Some(61_000)));
        assert_eq!(next.event["source"], "schedule");
        assert_eq!(retry_note(60_000), "Trying again in 1 minute.");
        assert_eq!(retry_note(900_000), "Trying again in 15 minutes.");
    }

    #[test]
    fn a_scheduled_fire_already_queued_is_the_retry() {
        let (mut s, i) = failed(0);
        let wake = new_run(&s.routines[0], json!({"source":"schedule"}), None);
        s.runs.push(wake);
        after_failure(&mut s, i, true, 1_000);
        assert_eq!(s.runs.len(), 2, "no second run of the same work");
        assert_eq!((s.runs[1].attempt, s.runs[0].retry_at), (1, Some(1_000)));
        assert_eq!(retry_note(0), "Trying again now.");
    }

    #[test]
    fn a_run_that_acted_or_ran_out_of_retries_waits_for_the_user() {
        for (attempt, safe) in [(0, false), (3, true)] {
            let (mut s, i) = failed(attempt);
            after_failure(&mut s, i, safe, 0);
            assert_eq!(s.runs.len(), 1, "no automatic retry");
            assert!(s.runs[0].retryable && s.runs[0].retry_at.is_none());
        }
    }

    #[test]
    fn a_paused_routine_is_not_retried_automatically() {
        let (mut s, i) = failed(0);
        s.routines[0].enabled = false;
        after_failure(&mut s, i, true, 0);
        assert_eq!(s.runs.len(), 1);
        assert!(s.runs[0].retryable);
    }

    #[test]
    fn only_the_latest_failure_keeps_its_event() {
        let (mut s, _) = failed(3);
        after_failure(&mut s, 0, false, 0);
        let mut second = s.runs[0].clone();
        second.id = "second".into();
        second.retryable = false;
        s.runs.push(second);
        assert_eq!(after_failure(&mut s, 1, false, 0), vec![s.runs[0].id.clone()]);
        assert!(!s.runs[0].retryable && s.runs[0].event.is_null());
        assert!(s.runs[1].retryable && !s.runs[1].event.is_null());
    }

    #[test]
    fn a_user_retry_needs_a_free_resumed_routine_and_deleting_it_withdraws_the_retry() {
        let dir = std::env::temp_dir().join(format!("codync-retry-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let hub = Hub::new(store, "retry-test".into(), Identity::load_or_create(&dir).unwrap(), "test".into(), 0);
        let (mut s, i) = failed(3);
        after_failure(&mut s, i, false, 0);
        let id = s.runs[i].id.clone();
        let mut running = new_run(&s.routines[0], Value::Null, None);
        running.status = Status::Running;
        s.runs.push(running);
        hub.routines
            .change(&hub.store, |state| {
                *state = s;
                Ok(())
            })
            .unwrap();
        let refused = |e: anyhow::Error| e.downcast::<Refused>().unwrap();
        assert_eq!(refused(hub.routines.retry(&hub, "bot", &id).unwrap_err()), Refused::Busy);
        hub.routines
            .change(&hub.store, |s| {
                s.runs[1].status = Status::Succeeded;
                s.routines[0].enabled = false;
                Ok(())
            })
            .unwrap();
        assert_eq!(refused(hub.routines.retry(&hub, "bot", &id).unwrap_err()), Refused::Paused);

        let notice = result_notice_id(&id);
        let data = json!({"text": "failed", "retry": true});
        hub.add_entry_once(&notice, &Lane::main("bot"), EntryKind::Notice, 0, &data).unwrap();
        assert!(hub.routines.remove(&hub, "other", "routine").is_err(), "another bot can't delete it");
        hub.routines.remove(&hub, "bot", "routine").unwrap();
        let state = hub.routines.state.locked();
        assert!(!state.runs[i].retryable && state.runs[i].event.is_null(), "the payload goes with the routine");
        assert_eq!(hub.store.entry(&notice).unwrap().data["retry"], false, "and so does the retry button");
    }
}
