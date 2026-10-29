//! Serialize registration with close so a late answer cannot resurrect capture.

use crate::candidates::Candidate;
use crate::stream::Session;
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
pub(super) struct Sessions(Mutex<HashMap<String, Arc<Mutex<State>>>>);

impl Sessions {
    pub(super) async fn answer(
        &self,
        id: String,
        offer: &str,
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
            State::Live(session) => return session.answer(offer).await,
            State::Closed => bail!("screen session closed"),
            State::Preparing => {}
        }
        let created = create.await;
        match created {
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

    pub(super) async fn candidate(&self, id: &str, candidate: &Candidate) -> Result<()> {
        let slot = {
            let sessions = self.0.lock().await;
            sessions.get(id).context("unknown screen session")?.clone()
        };
        let state = slot.lock().await;
        let State::Live(session) = &*state else {
            bail!("screen session is not ready");
        };
        session.candidate(candidate)
    }

    pub(super) async fn close(&self, id: &str) {
        let slot = {
            let mut sessions = self.0.lock().await;
            sessions.remove(id)
        };
        if let Some(slot) = slot {
            Self::close_slot(&slot).await;
        }
    }

    pub(super) async fn is_live(&self, id: &str, peer: &gst::Element) -> bool {
        let slot = {
            let sessions = self.0.lock().await;
            let Some(slot) = sessions.get(id) else {
                return false;
            };
            slot.clone()
        };
        let state = slot.lock().await;
        matches!(&*state, State::Live(session) if session.webrtc == *peer)
    }

    pub(super) async fn close_all(&self) {
        let slots = {
            let mut sessions = self.0.lock().await;
            sessions.drain().map(|(_, slot)| slot).collect::<Vec<_>>()
        };
        for slot in slots {
            Self::close_slot(&slot).await;
        }
    }

    pub(super) async fn close_peer(&self, id: &str, peer: &gst::Element, closed: impl FnOnce()) {
        let slot = {
            let sessions = self.0.lock().await;
            let Some(slot) = sessions.get(id) else { return };
            slot.clone()
        };
        let mut state = slot.lock().await;
        if !matches!(&*state, State::Live(session) if session.webrtc == *peer) {
            return;
        }
        if let State::Live(session) = std::mem::replace(&mut *state, State::Closed) {
            session.close();
        }
        let mut sessions = self.0.lock().await;
        if sessions
            .get(id)
            .is_some_and(|current| Arc::ptr_eq(current, &slot))
        {
            sessions.remove(id);
            // Emit before the ID can be registered again, preserving close/replacement ordering.
            closed();
        }
    }

    async fn close_slot(slot: &Mutex<State>) {
        let mut state = slot.lock().await;
        if let State::Live(session) = std::mem::replace(&mut *state, State::Closed) {
            session.close();
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use gst::prelude::*;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::oneshot;

    fn session() -> (Session, gst::Pipeline) {
        gst::init().unwrap();
        let pipeline = gst::parse::launch("videotestsrc is-live=true ! fakesink")
            .unwrap()
            .downcast::<gst::Pipeline>()
            .unwrap();
        pipeline.set_state(gst::State::Playing).unwrap();
        let session = Session {
            pipeline: pipeline.clone(),
            webrtc: gst::ElementFactory::make("webrtcbin").build().unwrap(),
            incoming: Some(std::sync::Mutex::default()),
        };
        (session, pipeline)
    }

    #[tokio::test]
    async fn close_during_answer_waits_and_stops_the_late_pipeline() {
        for all in [false, true] {
            let (session, pipeline) = session();
            let sessions = Arc::new(Sessions::default());
            let (started, starting) = oneshot::channel();
            let (finish, finishing) = oneshot::channel();
            let answering = tokio::spawn({
                let sessions = sessions.clone();
                async move {
                    sessions
                        .answer("viewer".into(), "offer", async {
                            started.send(()).unwrap();
                            let finished = finishing.await;
                            finished.unwrap();
                            Ok((session, "answer".into()))
                        })
                        .await
                }
            });
            let started = starting.await;
            started.unwrap();
            let mut closing = tokio::spawn({
                let sessions = sessions.clone();
                async move {
                    if all {
                        sessions.close_all().await;
                    } else {
                        sessions.close("viewer").await;
                    }
                }
            });
            let early = tokio::time::timeout(Duration::from_millis(25), &mut closing).await;
            assert!(
                early.is_err(),
                "close cannot acknowledge while an answer can still register"
            );
            finish.send(()).unwrap();
            let answer = answering.await;
            assert_eq!(answer.unwrap().unwrap(), "answer");
            let closed = closing.await;
            closed.unwrap();
            assert_eq!(pipeline.current_state(), gst::State::Null);
            let candidate = sessions.candidate("viewer", &Candidate::Complete).await;
            assert!(
                candidate.is_err(),
                "the closed session was not registered later"
            );
        }
    }

    #[tokio::test]
    async fn a_pending_answer_does_not_block_another_viewer_from_connecting_or_closing() {
        let (slow, _) = session();
        let sessions = Arc::new(Sessions::default());
        let (started, starting) = oneshot::channel();
        let (finish, finishing) = oneshot::channel();
        let pending = tokio::spawn({
            let sessions = sessions.clone();
            async move {
                sessions
                    .answer("slow".into(), "offer", async {
                        started.send(()).unwrap();
                        let finished = finishing.await;
                        finished.unwrap();
                        Ok((slow, "slow answer".into()))
                    })
                    .await
            }
        });
        let started = starting.await;
        started.unwrap();
        let (fast, pipeline) = session();
        let answered = tokio::time::timeout(
            Duration::from_secs(1),
            sessions.answer("fast".into(), "offer", async {
                Ok((fast, "fast answer".into()))
            }),
        )
        .await;
        assert_eq!(answered.unwrap().unwrap(), "fast answer");
        let closed = tokio::time::timeout(Duration::from_secs(1), sessions.close("fast")).await;
        closed.unwrap();
        assert_eq!(pipeline.current_state(), gst::State::Null);
        finish.send(()).unwrap();
        let finished = pending.await;
        finished.unwrap().unwrap();
        sessions.close_all().await;
    }
}
