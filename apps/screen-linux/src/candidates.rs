//! Ordered ICE notifications from GStreamer's signaling callbacks.

use crate::Helper;
use anyhow::{Result, bail};
use gst::prelude::*;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

const MAX_CANDIDATES: usize = 128;
const MAX_CANDIDATE_BYTES: usize = 4096;
const MAX_MEDIA_INDEX: u64 = 16;

#[derive(Default)]
pub(super) struct Candidates {
    count: usize,
    complete: bool,
}

impl Candidates {
    pub(super) fn accept(&mut self, candidate: &Value) -> Result<()> {
        if self.complete {
            bail!("screen candidates already complete");
        }
        match candidate["type"].as_str() {
            Some("complete") => self.complete = true,
            Some("candidate") => {
                let sdp = candidate["candidate"].as_str().unwrap_or_default();
                if !sdp.starts_with("candidate:")
                    || sdp.len() > MAX_CANDIDATE_BYTES
                    || sdp.contains(['\r', '\n'])
                    || candidate["sdpMLineIndex"]
                        .as_u64()
                        .is_none_or(|index| index > MAX_MEDIA_INDEX)
                {
                    bail!("invalid screen candidate");
                }
                if self.count >= MAX_CANDIDATES {
                    bail!("too many screen candidates");
                }
                self.count += 1;
            }
            _ => bail!("invalid screen candidate event"),
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
        move |candidate: Value| {
            let Some(helper) = helper.upgrade() else {
                return;
            };
            // Both callback kinds enter one FIFO synchronously; no spawned task can reorder them.
            let mut state = state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Err(error) = state.accept(&candidate) {
                tracing::warn!(%error, "screen candidate exchange failed");
                let _guard = runtime.enter();
                helper.ended(&id);
                return;
            }
            if helper.out.send(json!({"jsonrpc": "2.0", "method": "candidate", "params": {"session": id, "candidate": candidate}})).is_err() {
                let _guard = runtime.enter();
                helper.ended(&id);
            }
        }
    });
    webrtc.connect("on-ice-candidate", false, {
        let emit = emit.clone();
        move |values| {
            match (values[1].get::<u32>(), values[2].get::<String>()) {
                (Ok(index), Ok(candidate)) => emit(
                    json!({"type": "candidate", "candidate": candidate, "sdpMLineIndex": index}),
                ),
                _ => emit(Value::Null),
            }
            None
        }
    });
    webrtc.connect_notify(Some("ice-gathering-state"), move |peer, _| {
        if peer.property::<gst_webrtc::WebRTCICEGatheringState>("ice-gathering-state")
            == gst_webrtc::WebRTCICEGatheringState::Complete
        {
            emit(json!({"type": "complete"}));
        }
    });
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn completion_follows_all_candidates_and_bounds_are_enforced() {
        let mut stream = Candidates::default();
        let candidate = json!({"type": "candidate", "candidate": "candidate:1 1 UDP 1 192.0.2.1 5000 typ relay", "sdpMLineIndex": 0});
        for _ in 0..128 {
            stream.accept(&candidate).unwrap();
        }
        assert!(stream.accept(&candidate).is_err());
        stream.accept(&json!({"type": "complete"})).unwrap();
        assert!(stream.accept(&candidate).is_err());
        assert!(stream.accept(&json!({"type": "complete"})).is_err());
    }
}
