//! Ordered ICE candidates: out from GStreamer's signaling callbacks, in from the host.

use crate::Helper;
use anyhow::{Result, anyhow, bail};
use gst::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{Arc, Mutex};

const MAX_CANDIDATES: usize = 128;
const MAX_CANDIDATE_BYTES: usize = 4096;
const MAX_MEDIA_INDEX: u32 = 16;

/// One candidate event on the wire, as the host's `Candidate` (this helper needs no `sdpMid`).
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(super) enum Candidate {
    Candidate {
        candidate: String,
        #[serde(rename = "sdpMLineIndex")]
        sdp_m_line_index: u32,
    },
    Complete,
}

/// One direction of the exchange: at most `MAX_CANDIDATES` valid candidates, then one completion.
#[derive(Default)]
pub(super) struct Candidates {
    count: usize,
    complete: bool,
}

impl Candidates {
    pub(super) fn accept(&mut self, candidate: &Candidate) -> Result<()> {
        if self.complete {
            bail!("screen candidates already complete");
        }
        match candidate {
            Candidate::Complete => self.complete = true,
            Candidate::Candidate {
                candidate,
                sdp_m_line_index,
            } => {
                if !candidate.starts_with("candidate:")
                    || candidate.len() > MAX_CANDIDATE_BYTES
                    || candidate.contains(['\r', '\n'])
                    || *sdp_m_line_index > MAX_MEDIA_INDEX
                {
                    bail!("invalid screen candidate");
                }
                if self.count >= MAX_CANDIDATES {
                    bail!("too many screen candidates");
                }
                self.count += 1;
            }
        }
        Ok(())
    }
}

pub(super) fn observe(webrtc: &gst::Element, id: &str, helper: &Arc<Helper>) {
    let state = Arc::new(Mutex::new(Candidates::default()));
    let runtime = tokio::runtime::Handle::current();
    let emit = Arc::new({
        let id = id.to_owned();
        let helper = Arc::downgrade(helper);
        move |candidate: Result<Candidate>| {
            let Some(helper) = helper.upgrade() else {
                return;
            };
            // Both callback kinds enter one FIFO synchronously; no spawned task can reorder them.
            let mut state = state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let sent = candidate.and_then(|candidate| {
                state.accept(&candidate)?;
                helper.notify("candidate", json!({"session": id, "candidate": candidate}))
            });
            if let Err(error) = sent {
                tracing::warn!(%error, "screen candidate exchange failed");
                let _guard = runtime.enter();
                helper.ended(&id);
            }
        }
    });
    webrtc.connect("on-ice-candidate", false, {
        let emit = emit.clone();
        move |values| {
            match (values[1].get::<u32>(), values[2].get::<String>()) {
                // webrtcbin emits an empty end-of-candidates marker for each
                // transport. Aggregate gathering-state Complete below ends the
                // wire exchange only after every transport has finished.
                (Ok(_), Ok(candidate)) if candidate.is_empty() => {}
                (Ok(sdp_m_line_index), Ok(candidate)) => emit(Ok(Candidate::Candidate {
                    candidate,
                    sdp_m_line_index,
                })),
                _ => emit(Err(anyhow!("malformed native ICE candidate"))),
            }
            None
        }
    });
    webrtc.connect_notify(Some("ice-gathering-state"), move |peer, _| {
        if peer.property::<gst_webrtc::WebRTCICEGatheringState>("ice-gathering-state")
            == gst_webrtc::WebRTCICEGatheringState::Complete
        {
            emit(Ok(Candidate::Complete));
        }
    });
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::stream::Session;
    use serde_json::Value;
    use tokio::sync::{Mutex as AsyncMutex, mpsc};

    fn wire(sdp: &str) -> Candidate {
        Candidate::Candidate {
            candidate: sdp.to_owned(),
            sdp_m_line_index: 0,
        }
    }

    fn fixture() -> (Arc<Helper>, mpsc::UnboundedReceiver<Value>, Session) {
        gst::init().unwrap();
        let (out, events) = mpsc::unbounded_channel();
        let helper = Arc::new(Helper {
            portal: Arc::new(AsyncMutex::new(None)),
            last_status: AsyncMutex::new(None),
            sessions: crate::sessions::Sessions::default(),
            out,
            rt: tokio::runtime::Handle::current(),
        });
        let session = Session {
            pipeline: gst::Pipeline::new(),
            webrtc: gst::ElementFactory::make("webrtcbin").build().unwrap(),
            incoming: Some(Mutex::default()),
        };
        (helper, events, session)
    }

    #[tokio::test]
    async fn native_transport_end_markers_do_not_close_the_viewer_or_finish_other_candidates() {
        let (helper, mut events, session) = fixture();
        let peer = session.webrtc.clone();
        observe(&peer, "viewer", &helper);
        let registered = helper
            .sessions
            .answer("viewer".into(), "offer", async {
                Ok((session, "answer".into()))
            })
            .await;
        registered.unwrap();
        let sdp = "candidate:1 1 UDP 1 192.0.2.1 5000 typ host";
        peer.emit_by_name::<()>("on-ice-candidate", &[&0u32, &sdp]);
        assert_eq!(
            events.try_recv().unwrap()["params"]["candidate"]["candidate"],
            sdp
        );

        // One media transport can finish while another still emits candidates.
        peer.emit_by_name::<()>("on-ice-candidate", &[&0u32, &""]);
        peer.emit_by_name::<()>("on-ice-candidate", &[&1u32, &""]);
        tokio::task::yield_now().await;
        assert!(
            events.try_recv().is_err(),
            "native end markers are not wire candidates or global completion"
        );
        let live = helper.sessions.is_live("viewer", &peer).await;
        assert!(live, "a native gathering end marker must not stop media");
        peer.emit_by_name::<()>("on-ice-candidate", &[&1u32, &sdp]);
        assert_eq!(
            events.try_recv().unwrap()["params"]["candidate"]["sdpMLineIndex"],
            1
        );
        helper.sessions.close_all().await;
    }

    #[tokio::test]
    async fn native_gathering_exports_one_global_completion_after_all_candidates() {
        let (helper, mut events, session) = fixture();
        session.pipeline.add(&session.webrtc).unwrap();
        session.pipeline.set_state(gst::State::Ready).unwrap();
        let peer = session.webrtc.clone();
        observe(&peer, "viewer", &helper);
        let offer = "v=0\r\no=- 0 0 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n\
            a=group:BUNDLE 0\r\nm=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\n\
            c=IN IP4 0.0.0.0\r\na=mid:0\r\na=setup:actpass\r\na=sctp-port:5000\r\n\
            a=ice-ufrag:test\r\na=ice-pwd:testpasswordforthedataoffer\r\n\
            a=fingerprint:sha-256 00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00\r\n";
        let answered = session.answer(offer).await;
        let answer = answered.unwrap();
        let registered = helper
            .sessions
            .answer("viewer".into(), offer, async { Ok((session, answer)) })
            .await;
        registered.unwrap();
        let gathering = async {
            let mut count = 0;
            loop {
                let received = events.recv().await;
                let event = received.unwrap();
                assert_eq!(
                    event["method"], "candidate",
                    "gathering must not close the media session"
                );
                let candidate = &event["params"]["candidate"];
                if candidate["type"] == "complete" {
                    break;
                }
                assert_eq!(candidate["type"], "candidate");
                assert!(
                    candidate["candidate"]
                        .as_str()
                        .unwrap()
                        .starts_with("candidate:")
                );
                count += 1;
            }
            assert!(count > 0, "the native peer must actually gather candidates");
        };
        let gathered = tokio::time::timeout(std::time::Duration::from_secs(10), gathering).await;
        gathered.unwrap();
        assert_eq!(
            peer.property::<gst_webrtc::WebRTCICEGatheringState>("ice-gathering-state"),
            gst_webrtc::WebRTCICEGatheringState::Complete
        );
        tokio::task::yield_now().await;
        assert!(
            events.try_recv().is_err(),
            "completion is exported once, after every native candidate"
        );
        let live = helper.sessions.is_live("viewer", &peer).await;
        assert!(live, "finishing native gathering must not close media");
        helper.sessions.close_all().await;
    }

    #[test]
    fn native_end_markers_do_not_relax_wire_candidate_validation() {
        for sdp in ["", " ", "candidate:1\r\n"] {
            let mut stream = Candidates::default();
            assert!(stream.accept(&wire(sdp)).is_err());
        }
    }

    #[test]
    fn completion_follows_all_candidates_and_bounds_are_enforced() {
        let mut stream = Candidates::default();
        let candidate = wire("candidate:1 1 UDP 1 192.0.2.1 5000 typ relay");
        for _ in 0..128 {
            stream.accept(&candidate).unwrap();
        }
        assert!(stream.accept(&candidate).is_err());
        stream.accept(&Candidate::Complete).unwrap();
        assert!(stream.accept(&candidate).is_err());
        assert!(stream.accept(&Candidate::Complete).is_err());
    }
}
