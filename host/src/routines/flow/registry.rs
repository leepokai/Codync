//! The flows running now, so cancelling a routine, a bot or one run can stop their steps.

use crate::LockExt;
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::watch;

struct Active {
    routine_id: String,
    owner: String,
    cancel: watch::Sender<bool>,
}

#[derive(Default)]
pub struct Registry(Mutex<HashMap<String, Active>>);

/// Keeps a run registered while its flow executes.
pub struct Running<'a> {
    registry: &'a Registry,
    run_id: String,
    pub cancel: watch::Receiver<bool>,
}

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.registry.0.locked().remove(&self.run_id);
    }
}

impl Registry {
    pub fn register(&self, run_id: &str, routine_id: &str, owner: &str) -> Running<'_> {
        let (cancel, receiver) = watch::channel(false);
        let active = Active { routine_id: routine_id.to_owned(), owner: owner.to_owned(), cancel };
        self.0.locked().insert(run_id.to_owned(), active);
        Running { registry: self, run_id: run_id.to_owned(), cancel: receiver }
    }

    fn cancel_where(&self, matches: impl Fn(&Active) -> bool) -> usize {
        let runs = self.0.locked();
        let hit: Vec<_> = runs.values().filter(|a| matches(a)).collect();
        for active in &hit {
            active.cancel.send_replace(true);
        }
        hit.len()
    }

    /// Cancels one run's flow; whether it was running.
    #[cfg(test)]
    pub fn cancel_run(&self, run_id: &str) -> bool {
        self.0.locked().get(run_id).map(|a| a.cancel.send_replace(true)).is_some()
    }

    pub fn cancel_routine(&self, routine_id: &str) -> usize {
        self.cancel_where(|a| a.routine_id == routine_id)
    }

    /// Cancels the flows that start from `bot`'s routines.
    pub fn cancel_bot(&self, bot: &str) -> usize {
        self.cancel_where(|a| a.owner == bot)
    }

    #[cfg(test)]
    pub fn running(&self) -> usize {
        self.0.locked().len()
    }
}
