//! WebRTC SDP and ICE negotiation.

use crate::candidates::Candidate;
use crate::stream::Session;
use anyhow::{Context, Result, anyhow, bail};
use gst::prelude::*;
use std::time::Duration;
use tokio::sync::oneshot;

impl Session {
    /// Trickle sends the original answer immediately; complete-SDP peers wait for gathering.
    pub async fn answer(&self, offer: &str) -> Result<String> {
        let sdp = gst_sdp::SDPMessage::parse_buffer(offer.as_bytes())
            .map_err(|_| anyhow!("bad offer"))?;
        let offer =
            gst_webrtc::WebRTCSessionDescription::new(gst_webrtc::WebRTCSDPType::Offer, sdp);
        let (p, done) = promise();
        self.webrtc
            .emit_by_name::<()>("set-remote-description", &[&offer, &p]);
        done.await?;
        let (p, done) = promise();
        self.webrtc
            .emit_by_name::<()>("create-answer", &[&None::<gst::Structure>, &p]);
        let reply = done.await?;
        let reply = reply.ok_or_else(|| anyhow!("no answer"))?;
        let answer = reply
            .get::<gst_webrtc::WebRTCSessionDescription>("answer")
            .map_err(|_| anyhow!("no answer"))?;
        let (p, done) = promise();
        self.webrtc
            .emit_by_name::<()>("set-local-description", &[&answer, &p]);
        done.await?;
        // Negotiate before the encoder chooses fixed caps. Starting first can
        // produce High-profile caps that reject a phone's Baseline offer.
        self.pipeline
            .set_state(gst::State::Playing)
            .context("starting the video pipeline")?;
        if self.incoming.is_some() {
            return answer
                .sdp()
                .as_text()
                .map_err(|_| anyhow!("couldn't write the answer"));
        }
        // Complete-SDP clients need all transports, including late TURN TLS/443 candidates.
        for _ in 0..200 {
            if self
                .webrtc
                .property::<gst_webrtc::WebRTCICEGatheringState>("ice-gathering-state")
                == gst_webrtc::WebRTCICEGatheringState::Complete
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let local = self
            .webrtc
            .property::<Option<gst_webrtc::WebRTCSessionDescription>>("local-description");
        let local = local.unwrap_or(answer);
        local
            .sdp()
            .as_text()
            .map_err(|_| anyhow!("couldn't write the answer"))
    }

    pub fn candidate(&self, candidate: &Candidate) -> Result<()> {
        let mut incoming = self
            .incoming
            .as_ref()
            .context("screen session does not support trickle ICE")?
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        incoming.accept(candidate)?;
        match candidate {
            // 1.22 supports end-of-candidates without add-ice-candidate-full.
            Candidate::Complete => self
                .webrtc
                .emit_by_name::<()>("add-ice-candidate", &[&0u32, &None::<String>]),
            Candidate::Candidate {
                candidate,
                sdp_m_line_index,
            } => self
                .webrtc
                .emit_by_name::<()>("add-ice-candidate", &[sdp_m_line_index, candidate]),
        }
        Ok(())
    }
}

/// Convert WebRTC's ICE URL syntax to GStreamer's URI syntax without exposing credentials.
pub(super) fn ice_uri(
    url: &str,
    username: Option<&str>,
    credential: Option<&str>,
) -> Result<String> {
    let (scheme, address) = url.split_once(':').context("invalid ICE URL")?;
    if scheme == "stun" {
        return Ok(format!("stun://{address}"));
    }
    if !matches!(scheme, "turn" | "turns") {
        bail!("unsupported ICE scheme");
    }
    let username = username.context("missing TURN username")?;
    let credential = credential.context("missing TURN credential")?;
    let escape = |s: &str| -> String {
        use std::fmt::Write as _;
        s.bytes().fold(String::new(), |mut out, byte| {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                out.push(char::from(byte));
            } else {
                let _ = write!(out, "%{byte:02X}");
            }
            out
        })
    };
    Ok(format!(
        "{scheme}://{}:{}@{address}",
        escape(username),
        escape(credential)
    ))
}

fn promise() -> (gst::Promise, oneshot::Receiver<Option<gst::Structure>>) {
    let (tx, rx) = oneshot::channel();
    let p = gst::Promise::with_change_func(move |reply| {
        let _ = tx.send(reply.ok().flatten().map(ToOwned::to_owned));
    });
    (p, rx)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn turn_uri_preserves_transport_and_escapes_credentials() {
        assert_eq!(
            ice_uri(
                "turns:turn.cloudflare.com:443?transport=tcp",
                Some("a:b"),
                Some("c+/@")
            )
            .unwrap(),
            "turns://a%3Ab:c%2B%2F%40@turn.cloudflare.com:443?transport=tcp"
        );
        assert_eq!(
            ice_uri("stun:stun.cloudflare.com:3478", None, None).unwrap(),
            "stun://stun.cloudflare.com:3478"
        );
        assert!(ice_uri("turn:turn.cloudflare.com:3478", None, None).is_err());
    }
}
