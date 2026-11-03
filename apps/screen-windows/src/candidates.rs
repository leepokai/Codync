//! Ordered ICE candidates on the wire between the host and this helper, in both directions.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

const MAX_CANDIDATES: usize = 128;
const MAX_CANDIDATE_BYTES: usize = 4096;
const MAX_MEDIA_INDEX: u32 = 16;

/// One candidate event on the wire, as the host's `Candidate`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Candidate {
    Candidate {
        candidate: String,
        #[serde(rename = "sdpMLineIndex")]
        sdp_m_line_index: u32,
        #[serde(default, rename = "sdpMid", skip_serializing_if = "Option::is_none")]
        sdp_mid: Option<String>,
    },
    Complete,
}

/// One direction of the exchange: at most `MAX_CANDIDATES` valid candidates, then one completion.
#[derive(Default)]
pub struct Candidates {
    count: usize,
    complete: bool,
}

impl Candidates {
    pub fn accept(&mut self, candidate: &Candidate) -> Result<()> {
        if self.complete {
            bail!("screen candidates already complete");
        }
        match candidate {
            Candidate::Complete => self.complete = true,
            Candidate::Candidate {
                candidate,
                sdp_m_line_index,
                ..
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn wire(candidate: &str) -> Candidate {
        Candidate::Candidate {
            candidate: candidate.to_owned(),
            sdp_m_line_index: 0,
            sdp_mid: None,
        }
    }

    #[test]
    fn parses_the_hosts_wire_format() {
        let parsed: Candidate = serde_json::from_value(
            json!({"type": "candidate", "candidate": "candidate:1 1 UDP 1 192.0.2.1 5000 typ host", "sdpMLineIndex": 0, "sdpMid": "0"}),
        )
        .unwrap();
        assert!(matches!(
            parsed,
            Candidate::Candidate {
                sdp_mid: Some(_),
                ..
            }
        ));
        assert_eq!(
            serde_json::from_value::<Candidate>(json!({"type": "complete"})).unwrap(),
            Candidate::Complete
        );
        let sent = serde_json::to_value(wire("candidate:1")).unwrap();
        assert_eq!(
            sent,
            json!({"type": "candidate", "candidate": "candidate:1", "sdpMLineIndex": 0})
        );
    }

    #[test]
    fn bounds_each_direction() {
        let mut seen = Candidates::default();
        assert!(seen.accept(&wire("not a candidate")).is_err());
        assert!(seen.accept(&wire("candidate:1\r\nm=video")).is_err());
        assert!(
            seen.accept(&Candidate::Candidate {
                candidate: "candidate:1".into(),
                sdp_m_line_index: 99,
                sdp_mid: None
            })
            .is_err()
        );
        for _ in 0..MAX_CANDIDATES {
            seen.accept(&wire("candidate:1")).unwrap();
        }
        assert!(seen.accept(&wire("candidate:1")).is_err());
        seen.accept(&Candidate::Complete).unwrap();
        assert!(seen.accept(&Candidate::Complete).is_err());
    }
}
