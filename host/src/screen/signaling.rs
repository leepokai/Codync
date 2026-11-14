//! One ordered, private ICE exchange per prepared viewer.

use super::{Screen, Viewer, ViewerState};
use crate::LockExt;
use anyhow::{Context, Result, bail};
use futures::{Stream, StreamExt as _, stream};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::{mpsc, watch};

const MAX_CANDIDATES: usize = 128;
const MAX_CANDIDATE_BYTES: usize = 4096;
const MAX_MEDIA_INDEX: u32 = 16;
const MAX_MID_BYTES: usize = 256;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Candidate {
    Candidate {
        candidate: String,
        #[serde(rename = "sdpMLineIndex")]
        sdp_m_line_index: u32,
        #[serde(rename = "sdpMid", default)]
        sdp_mid: Option<String>,
    },
    Complete,
}

impl Candidate {
    fn validate(&self) -> Result<()> {
        if let Self::Candidate { candidate, sdp_m_line_index, sdp_mid } = self
            && (!candidate.starts_with("candidate:")
                || candidate.len() > MAX_CANDIDATE_BYTES
                || *sdp_m_line_index > MAX_MEDIA_INDEX
                || sdp_mid.as_ref().is_some_and(|mid| mid.len() > MAX_MID_BYTES)
                || candidate.contains(['\r', '\n']))
        {
            bail!("invalid screen candidate");
        }
        Ok(())
    }
}

#[derive(Default)]
struct Progress {
    count: usize,
    complete: bool,
}

impl Progress {
    fn accept(&mut self, candidate: &Candidate) -> Result<()> {
        candidate.validate()?;
        if self.complete {
            bail!("screen candidates already complete");
        }
        match candidate {
            Candidate::Complete => self.complete = true,
            Candidate::Candidate { .. } => {
                if self.count >= MAX_CANDIDATES {
                    bail!("too many screen candidates");
                }
                self.count += 1;
            }
        }
        Ok(())
    }
}

#[derive(Default, PartialEq, Eq)]
enum Offer {
    #[default]
    Prepared,
    Answering,
    Answered,
}

pub(super) struct Signaling {
    #[cfg(any(unix, test))]
    tx: mpsc::Sender<Value>,
    rx: Option<mpsc::Receiver<Value>>,
    failure: watch::Sender<Option<String>>,
    offer: Offer,
    incoming: Progress,
    #[cfg(any(unix, test))]
    outgoing: Progress,
    uploading: bool,
    detached: bool,
    outgoing_complete_delivered: bool,
}

impl Signaling {
    pub(super) fn new() -> Self {
        let (tx, rx) = mpsc::channel(MAX_CANDIDATES + 1);
        // Windows has no helper socket to produce candidates yet.
        #[cfg(not(any(unix, test)))]
        drop(tx);
        let (failure, _) = watch::channel(None);
        Self {
            #[cfg(any(unix, test))]
            tx,
            rx: Some(rx),
            failure,
            offer: Offer::Prepared,
            incoming: Progress::default(),
            #[cfg(any(unix, test))]
            outgoing: Progress::default(),
            uploading: false,
            detached: false,
            outgoing_complete_delivered: false,
        }
    }

    pub(super) fn fail(&self, message: &str) {
        self.failure.send_if_modified(|reason| {
            if reason.is_some() {
                return false;
            }
            *reason = Some(message.to_owned());
            true
        });
    }

    fn check(&self) -> Result<()> {
        if let Some(reason) = self.failure.borrow().as_ref() {
            bail!("{reason}");
        }
        Ok(())
    }

    pub(super) fn begin_offer(&mut self) -> Result<()> {
        self.check()?;
        if self.rx.is_some() {
            bail!("open screenCandidates before offering");
        }
        if self.offer != Offer::Prepared {
            bail!("screen session already offered");
        }
        self.offer = Offer::Answering;
        Ok(())
    }

    pub(super) fn answered(&mut self) -> Result<()> {
        self.check()?;
        self.offer = Offer::Answered;
        Ok(())
    }

    fn complete(&self) -> bool {
        self.incoming.complete && self.outgoing_complete_delivered && !self.uploading
    }
}

impl Viewer {
    /// The viewer is done: watchers close it and its candidate stream reports `reason`.
    fn fail(&mut self, reason: &str) {
        self.state = ViewerState::Failed;
        if let Some(signaling) = &self.signaling {
            signaling.fail(reason);
        }
    }

    fn signaling(&mut self) -> Result<&mut Signaling> {
        if self.expired(crate::store::now_ms()) {
            bail!("screen session expired");
        }
        let signaling = self.signaling.as_mut().context("screen session does not support trickle ICE")?;
        if signaling.detached {
            bail!("screen signaling disconnected");
        }
        Ok(signaling)
    }
}

impl Screen {
    pub fn candidates(
        self: &Arc<Self>,
        session: &str,
        owner: &str,
    ) -> Result<impl Stream<Item = Value> + Send + 'static> {
        self.link()?;
        let (rx, failure) = {
            let mut sessions = self.sessions.locked();
            let viewer = sessions.get_mut(session).filter(|v| v.owner == owner).context("unknown screen session")?;
            let signaling = viewer.signaling()?;
            signaling.check()?;
            let rx = signaling.rx.take().context("screen candidates already subscribed")?;
            (rx, signaling.failure.subscribe())
        };
        let guard = Subscription { screen: self.clone(), session: session.to_owned() };
        // Ends right after reporting a failure, without draining possibly stale candidates.
        let events = stream::unfold((Some((rx, failure)), guard), |(live, guard)| async move {
            let (mut rx, mut failure) = live?;
            let event = tokio::select! {
                biased;
                reason = failure.wait_for(Option::is_some) => {
                    let message = reason.ok()?.clone().unwrap_or_default();
                    return Some((json!({"type": "error", "message": message}), (None, guard)));
                }
                event = rx.recv() => event?,
            };
            if event["type"] == "complete" {
                let mut sessions = guard.screen.sessions.locked();
                if let Some(signaling) = sessions.get_mut(&guard.session).and_then(|viewer| viewer.signaling.as_mut()) {
                    signaling.outgoing_complete_delivered = true;
                }
            }
            Some((event, (Some((rx, failure)), guard)))
        });
        Ok(stream::once(futures::future::ready(json!({"type": "ready"}))).chain(events))
    }

    pub async fn candidate(&self, session: &str, owner: &str, candidate: Candidate) -> Result<()> {
        let link = self.link()?;
        {
            let mut sessions = self.sessions.locked();
            let viewer = sessions.get_mut(session).filter(|v| v.owner == owner).context("unknown screen session")?;
            let signaling = viewer.signaling()?;
            signaling.check()?;
            if signaling.offer != Offer::Answered || signaling.uploading {
                bail!("screen candidate is not ready");
            }
            if let Err(error) = signaling.incoming.accept(&candidate) {
                viewer.fail(&error.to_string());
                return Err(error);
            }
            signaling.uploading = true;
        }
        let result = link.request("candidate", json!({"session": session, "candidate": candidate})).await;
        let mut sessions = self.sessions.locked();
        let viewer = sessions.get_mut(session).context("screen session closed")?;
        viewer.signaling()?.uploading = false;
        if let Err(error) = &result {
            viewer.fail(&error.to_string());
        }
        result?;
        viewer.signaling()?.check()
    }

    #[cfg(any(unix, test))]
    pub(super) fn helper_candidate(&self, session: &str, candidate: Value) {
        let mut sessions = self.sessions.locked();
        let Some(viewer) = sessions.get_mut(session) else { return };
        let Some(signaling) = viewer.signaling.as_mut() else { return };
        if signaling.detached {
            return;
        }
        let result = serde_json::from_value::<Candidate>(candidate.clone())
            .map_err(anyhow::Error::from)
            .and_then(|candidate| signaling.outgoing.accept(&candidate));
        if let Err(error) = result {
            viewer.fail(&error.to_string());
            return;
        }
        if signaling.offer == Offer::Prepared || signaling.tx.try_send(candidate).is_err() {
            viewer.fail("screen candidate stream overflow or closed");
        }
    }

    pub(super) fn fail_signaling(&self, session: &str, message: &str) {
        if let Some(viewer) = self.sessions.locked().get_mut(session) {
            viewer.fail(message);
        }
    }

    /// Candidate delivery ends with the channel; established media has its own lifetime.
    fn signaling_disconnected(&self, session: &str) -> bool {
        let mut sessions = self.sessions.locked();
        let Some(viewer) = sessions.get_mut(session) else { return false };
        #[cfg(any(unix, test))]
        let connected = viewer.media_connected();
        #[cfg(not(any(unix, test)))]
        let connected = false;
        let Some(signaling) = viewer.signaling.as_mut() else { return false };
        if viewer.state != ViewerState::Failed && (connected || signaling.complete()) {
            signaling.detached = true;
            return false;
        }
        viewer.fail("screen signaling disconnected");
        true
    }
}

struct Subscription {
    screen: Arc<Screen>,
    session: String,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if !self.screen.signaling_disconnected(&self.session) {
            return;
        }
        let screen = self.screen.clone();
        let session = self.session.clone();
        tokio::spawn(async move {
            if let Err(error) = screen.close(&session).await {
                tracing::warn!(error = format!("{error:#}"), "couldn't close screen signaling");
            }
        });
    }
}

#[cfg(test)]
mod tests;
