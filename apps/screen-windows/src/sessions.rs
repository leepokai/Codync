//! Serialize registration with close so a late answer cannot resurrect capture.

use crate::candidates::Candidate;
use crate::peer::Session;
use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::Mutex;

enum State {
    Preparing,
    Live(Session),
    Closed,
}

#[derive(Default)]
pub struct Sessions(Mutex<HashMap<String, Arc<Mutex<State>>>>);

impl Sessions {
    /// Creates the viewer's session once; a second offer for the same id is refused.
    pub async fn answer(
        &self,
        id: String,
        create: impl Future<Output = Result<(Session, String)>>,
    ) -> Result<String> {
        let slot = {
            let mut sessions = self.0.lock().await;
            sessions
                .entry(id)
                .or_insert_with(|| Arc::new(Mutex::new(State::Preparing)))
                .clone()
        };
        // Only this viewer waits for creation; other viewers can negotiate and close concurrently.
        let mut state = slot.lock().await;
        match &*state {
            State::Live(_) => bail!("screen session already offered"),
            State::Closed => bail!("screen session closed"),
            State::Preparing => {}
        }
        match create.await {
            Ok((session, answer)) => {
                *state = State::Live(session);
                Ok(answer)
            }
            Err(error) => {
                *state = State::Closed;
                Err(error)
            }
        }
    }

    pub async fn candidate(&self, id: &str, candidate: &Candidate) -> Result<()> {
        let slot = self
            .0
            .lock()
            .await
            .get(id)
            .context("unknown screen session")?
            .clone();
        let state = slot.lock().await;
        let State::Live(session) = &*state else {
            bail!("screen session is not ready")
        };
        session.candidate(candidate).await
    }

    pub async fn close(&self, id: &str) {
        let slot = self.0.lock().await.remove(id);
        if let Some(slot) = slot {
            close_slot(&slot).await;
        }
    }

    pub async fn close_all(&self) {
        let slots: Vec<_> = self.0.lock().await.drain().map(|(_, slot)| slot).collect();
        for slot in slots {
            close_slot(&slot).await;
        }
    }
}

async fn close_slot(slot: &Mutex<State>) {
    let mut state = slot.lock().await;
    if let State::Live(session) = std::mem::replace(&mut *state, State::Closed) {
        session.close().await;
    }
}
