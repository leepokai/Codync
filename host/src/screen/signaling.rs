//! One ordered, private ICE exchange per prepared viewer.

use super::{Screen, Viewer};
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
}

impl Viewer {
    fn signaling(&mut self) -> Result<&mut Signaling> {
        if self.ice.expires_at <= crate::store::now_ms() {
            bail!("screen session expired");
        }
        self.signaling.as_mut().context("screen session does not support trickle ICE")
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
            if !viewer.started && viewer.pending_until <= crate::store::now_ms() {
                bail!("screen session expired");
            }
            let signaling = viewer.signaling()?;
            signaling.check()?;
            let rx = signaling.rx.take().context("screen candidates already subscribed")?;
            (rx, signaling.failure.subscribe())
        };
        let guard = Subscription { screen: self.clone(), session: session.to_owned() };
        let events = stream::unfold((rx, failure, guard), |(mut rx, mut failure, guard)| async move {
            let reason = failure.borrow().clone();
            if let Some(message) = reason {
                return Some((json!({"type": "error", "message": message}), (rx, failure, guard)));
            }
            let event = tokio::select! {
                biased;
                result = failure.changed() => {
                    if result.is_err() { return None; }
                    return Some((json!({"type": "error", "message": failure.borrow().clone().unwrap_or_default()}), (rx, failure.clone(), guard)));
                }
                event = rx.recv() => event?,
            };
            Some((event, (rx, failure, guard)))
        });
        // Stop immediately after reporting failure, without draining possibly stale candidates.
        let events = events.scan(false, |ended, event| {
            let result = if *ended {
                None
            } else {
                *ended = event["type"] == "error";
                Some(event)
            };
            futures::future::ready(result)
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
                signaling.fail(&error.to_string());
                return Err(error);
            }
            signaling.uploading = true;
        }
        let result = link.request("candidate", json!({"session": session, "candidate": candidate})).await;
        let mut sessions = self.sessions.locked();
        let viewer = sessions.get_mut(session).context("screen session closed")?;
        let signaling = viewer.signaling()?;
        signaling.uploading = false;
        if let Err(error) = &result {
            signaling.fail(&error.to_string());
        }
        result?;
        signaling.check()
    }

    #[cfg(any(unix, test))]
    pub(super) fn helper_candidate(&self, session: &str, candidate: Value) {
        let mut sessions = self.sessions.locked();
        let Some(viewer) = sessions.get_mut(session) else { return };
        let Some(signaling) = viewer.signaling.as_mut() else { return };
        let result = serde_json::from_value::<Candidate>(candidate.clone())
            .map_err(anyhow::Error::from)
            .and_then(|candidate| signaling.outgoing.accept(&candidate));
        if let Err(error) = result {
            signaling.fail(&error.to_string());
            viewer.ice.expires_at = 0;
            return;
        }
        if signaling.offer == Offer::Prepared || signaling.tx.try_send(candidate).is_err() {
            signaling.fail("screen candidate stream overflow or closed");
            viewer.ice.expires_at = 0;
        }
    }

    pub(super) fn fail_signaling(&self, session: &str, message: &str) {
        if let Some(viewer) = self.sessions.locked().get_mut(session) {
            viewer.ice.expires_at = 0;
            if let Some(signaling) = &viewer.signaling {
                signaling.fail(message);
            }
        }
    }
}

struct Subscription {
    screen: Arc<Screen>,
    session: String,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.screen.fail_signaling(&self.session, "screen signaling disconnected");
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
