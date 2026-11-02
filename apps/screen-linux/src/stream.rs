//! Video: a PipeWire monitor stream (or the X11 root window) encoded as H.264 and
//! sent over GStreamer `webrtcbin` with negotiated trickle ICE, plus one-off JPEG stills.

use crate::Helper;
pub use crate::capture::Capture;
#[cfg(test)]
use crate::capture::capture_source;
use crate::clipboard;
use crate::negotiation::ice_uri;
use crate::portal::Display;
use crate::rtp::{payloader, sender};
use crate::video::{h264_format, h264_payload};
use anyhow::{Context, Result, anyhow, bail};
use base64::Engine as _;
use gst::prelude::*;
use serde_json::{Value, json};
use std::os::fd::IntoRawFd;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// Hardware encoders first; x264 is the always-there fallback.
const ENCODERS: &[&str] = &[
    "vah264lpenc",
    "vah264enc",
    "nvh264enc",
    "vaapih264enc",
    "x264enc",
    "openh264enc",
];
const BITRATE_KBPS: u32 = 8000;
/// x264 safety-net keyframe distance: 10 s at 60 fps, for viewers that never send a PLI.
const KEYFRAME_INTERVAL_FRAMES: u32 = 600;

pub struct Session {
    pub(super) pipeline: gst::Pipeline,
    pub(super) webrtc: gst::Element,
    /// The phone's candidates so far; `None` for a complete-SDP session.
    pub(super) incoming: Option<Mutex<crate::candidates::Candidates>>,
}

impl Session {
    pub fn new(
        id: String,
        display: &Display,
        capture: Capture,
        helper: &Arc<Helper>,
        config: &Value,
    ) -> Result<Self> {
        let payload = h264_payload(config["sdp"].as_str().context("missing offer")?)?;
        let pipeline = gst::Pipeline::new();
        // Capture and keepalive timestamps use the system clock. pipewiresrc only gained a
        // switch for this (`provide-clock`) in PipeWire 1.6; Ubuntu 24.04 ships 1.0.
        pipeline.use_clock(Some(&gst::SystemClock::obtain()));
        let fps = config["maxFramerate"].as_i64().unwrap_or(60).clamp(1, 60);
        let src = capture.source(display, i32::try_from(fps)?)?;
        let convert = make("videoconvert")?;
        let crop = make("videocrop")?;
        let scale = make("videoscale")?;
        let size = make("capsfilter")?;
        let rate = make("videorate")?;
        rate.set_property("max-rate", i32::try_from(fps)?);
        rate.set_property("drop-only", true);
        let convert2 = make("videoconvert")?;
        let format = h264_format()?;
        let queue = make("queue")?;
        queue.set_property_from_str("leaky", "downstream");
        queue.set_property("max-size-buffers", 2u32);
        let bitrate = config["maxBitrateBps"]
            .as_u64()
            .unwrap_or(u64::from(BITRATE_KBPS) * 1000)
            .clamp(100_000, u64::from(BITRATE_KBPS) * 1000);
        let encoder = encoder(u32::try_from(bitrate / 1000)?)?;
        let parse = make("h264parse")?;
        let [pay, rtp_caps] = payloader(payload)?;
        let webrtc = sender()?;
        if let Some(servers) = config["iceServers"].as_array() {
            for server in servers {
                for url in server["urls"].as_array().context("missing ICE URLs")? {
                    let url = url.as_str().context("invalid ICE URL")?;
                    let uri = ice_uri(
                        url,
                        server["username"].as_str(),
                        server["credential"].as_str(),
                    )?;
                    if url.starts_with("stun:") {
                        webrtc.set_property("stun-server", &uri);
                    } else if !webrtc.emit_by_name::<bool>("add-turn-server", &[&uri]) {
                        bail!("couldn't configure screen relay");
                    }
                }
            }
        }
        let chain = [
            &src, &convert, &crop, &scale, &size, &rate, &convert2, &format, &queue, &encoder,
            &parse, &pay, &rtp_caps, &webrtc,
        ];
        pipeline.add_many(chain)?;
        gst::Element::link_many(chain)?;

        // Input from the phone, applied in order.
        let (tx, mut rx) = mpsc::unbounded_channel::<Value>();
        let view = View {
            crop: crop.clone(),
            size: size.clone(),
            display: display.clone(),
        };
        let channels: Arc<Mutex<Vec<gst_webrtc::WebRTCDataChannel>>> = Arc::default();
        {
            let helper = helper.clone();
            let channels = channels.clone();
            let display = display.clone();
            tokio::spawn(async move {
                while let Some(msg) = rx.recv().await {
                    let reply = match msg["type"].as_str() {
                        Some("view") => Some(view.apply(&msg)),
                        Some("clipboard") => {
                            let x11 = helper.x11_display().await;
                            clipboard::set(msg["text"].as_str().unwrap_or_default(), x11.as_deref())
                                .await
                                .err()
                                .as_ref()
                                .map(error_msg)
                        }
                        _ => match helper.portal().await {
                            Ok(portal) => portal
                                .perform(&display, &msg)
                                .await
                                .err()
                                .as_ref()
                                .map(error_msg),
                            Err(error) => Some(error_msg(&error)),
                        },
                    };
                    if let Some(reply) = reply {
                        send(&channels, &reply);
                    }
                }
            });
        }
        webrtc.connect("on-data-channel", false, {
            let channels = channels.clone();
            move |values| {
                let channel = values[1].get::<gst_webrtc::WebRTCDataChannel>().ok()?;
                let tx = tx.clone();
                channel.connect("on-message-string", false, move |v| {
                    if let Some(msg) = v[1]
                        .get::<Option<String>>()
                        .ok()
                        .flatten()
                        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                    {
                        let _ = tx.send(msg);
                    }
                    None
                });
                if let Ok(mut list) = channels.lock() {
                    list.push(channel);
                }
                None
            }
        });
        let trickle = config["trickle"] == true;
        if trickle {
            crate::candidates::observe(&webrtc, &id, helper);
        }
        crate::connection::observe(&webrtc, id, helper);
        pipeline
            .set_state(gst::State::Ready)
            .context("preparing the video pipeline")?;
        Ok(Self {
            pipeline,
            webrtc,
            incoming: trickle.then(Mutex::default),
        })
    }

    pub fn close(self) {
        drop(self);
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Negotiation failure must stop capture too, before the session enters the helper's map.
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

/// Crop and output size for what the phone shows (see the iOS `ScreenViewport`).
struct View {
    crop: gst::Element,
    size: gst::Element,
    display: Display,
}

impl View {
    fn apply(&self, msg: &Value) -> Value {
        // Source frames can be larger than the logical size (HiDPI).
        let (fw, fh) = self
            .crop
            .static_pad("sink")
            .and_then(|p| p.current_caps())
            .and_then(|c| {
                let s = c.structure(0)?;
                Some((
                    f64::from(s.get::<i32>("width").ok()?),
                    f64::from(s.get::<i32>("height").ok()?),
                ))
            })
            .unwrap_or((self.display.width, self.display.height));
        let factor = fw / self.display.width;
        let crop = msg["crop"].as_array().and_then(|c| {
            let v: Vec<f64> = c.iter().filter_map(Value::as_f64).collect();
            (v.len() == 4 && v[2] >= 16.0 && v[3] >= 16.0)
                .then(|| (v[0].max(0.0), v[1].max(0.0), v[2], v[3]))
        });
        let (x, y, w, h) = crop.unwrap_or((0.0, 0.0, self.display.width, self.display.height));
        let px = |v: f64| to_i32(v * factor);
        self.crop.set_property("left", px(x));
        self.crop.set_property("top", px(y));
        self.crop
            .set_property("right", (to_i32(fw) - px(x + w)).max(0));
        self.crop
            .set_property("bottom", (to_i32(fh) - px(y + h)).max(0));
        if let (Some(long), Some(short)) = (msg["long"].as_f64(), msg["short"].as_f64()) {
            let (cw, ch) = (w * factor, h * factor);
            let fit = (long.min(3840.0) / cw.max(ch))
                .min(short.min(2160.0) / cw.min(ch))
                .min(1.0);
            let even = |v: f64| (to_i32(v * fit) / 2 * 2).max(2);
            self.size.set_property(
                "caps",
                gst::Caps::builder("video/x-raw")
                    .field("width", even(cw))
                    .field("height", even(ch))
                    .build(),
            );
        }
        json!({"type": "view", "display": self.display.id, "crop": crop.map(|(x, y, w, h)| json!([x, y, w, h]))})
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "screen sizes are a few thousand pixels"
)]
fn to_i32(v: f64) -> i32 {
    v.round() as i32
}

fn error_msg(e: &anyhow::Error) -> Value {
    json!({"type": "error", "message": format!("{e:#}")})
}

fn send(channels: &Mutex<Vec<gst_webrtc::WebRTCDataChannel>>, msg: &Value) {
    let Ok(list) = channels.lock() else { return };
    if let Some(ch) = list.iter().find(|c| c.label().as_deref() == Some("input"))
        && let Err(error) = ch.send_string_full(Some(&msg.to_string()))
    {
        tracing::debug!(%error, "couldn't reply on the data channel");
    }
}

pub(crate) fn make(name: &str) -> Result<gst::Element> {
    gst::ElementFactory::make(name)
        .build()
        .with_context(|| format!("GStreamer element {name} is missing"))
}

fn encoder(bitrate_kbps: u32) -> Result<gst::Element> {
    let name = ENCODERS
        .iter()
        .find(|n| gst::ElementFactory::find(n).is_some())
        .ok_or_else(|| {
            anyhow!("no H.264 encoder: install gstreamer1.0-plugins-ugly (x264) or a VA-API driver")
        })?;
    let e = make(name)?;
    match *name {
        "x264enc" => {
            e.set_property_from_str("tune", "zerolatency");
            e.set_property_from_str("speed-preset", "veryfast");
            e.set_property("bitrate", bitrate_kbps);
            // A screen keyframe is ~100x a delta frame. One every 120 frames (2 s at 60 fps) was a
            // periodic burst that overflowed shallow links; NACK and the phone's PLI cover recovery.
            e.set_property("key-int-max", KEYFRAME_INTERVAL_FRAMES);
        }
        "openh264enc" => e.set_property("bitrate", bitrate_kbps * 1000),
        _ => {
            if e.has_property("bitrate") {
                e.set_property_from_str("bitrate", &bitrate_kbps.to_string());
            }
        }
    }
    tracing::info!(encoder = name, "video encoder");
    Ok(e)
}

/// One still for agents: `width`×`height` JPEG, base64.
pub async fn screenshot(capture: Capture, d: &Display, width: u32, height: u32) -> Result<String> {
    let source = match &capture {
        Capture::PipeWire(_) => format!("pipewiresrc name=src path={} do-timestamp=true", d.node),
        Capture::X11(_) => "ximagesrc name=src use-damage=false show-pointer=true".to_owned(),
    };
    let desc = format!(
        "{source} ! videoconvert ! videoscale ! video/x-raw,width={width},height={height} ! jpegenc quality=80 ! appsink name=sink max-buffers=1"
    );
    let pipeline = gst::parse::launch(&desc)?
        .downcast::<gst::Pipeline>()
        .map_err(|_| anyhow!("bad pipeline"))?;
    let src = pipeline
        .by_name("src")
        .ok_or_else(|| anyhow!("no source"))?;
    match capture {
        Capture::PipeWire(fd) => src.set_property("fd", fd.into_raw_fd()),
        Capture::X11(name) => src.set_property("display-name", name),
    }
    let sink = pipeline
        .by_name("sink")
        .ok_or_else(|| anyhow!("no sink"))?
        .downcast::<gst_app::AppSink>()
        .map_err(|_| anyhow!("no sink"))?;
    pipeline.set_state(gst::State::Playing)?;
    let sample =
        tokio::task::spawn_blocking(move || sink.try_pull_sample(gst::ClockTime::from_seconds(3)))
            .await?;
    let _ = pipeline.set_state(gst::State::Null);
    let sample = sample.ok_or_else(|| anyhow!("the screen sent no frame"))?;
    let buffer = sample.buffer().ok_or_else(|| anyhow!("empty frame"))?;
    let map = buffer.map_readable()?;
    Ok(base64::engine::general_purpose::STANDARD.encode(map.as_slice()))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::candidates::Candidate;

    #[tokio::test]
    async fn trickle_answer_does_not_wait_for_candidate_gathering() {
        gst::init().unwrap();
        let pipeline = gst::parse::launch(
            "videotestsrc is-live=true ! videoconvert ! video/x-raw,format=I420 \
             ! x264enc tune=zerolatency ! h264parse ! rtph264pay pt=96 \
             ! application/x-rtp,media=video,encoding-name=H264,payload=96,clock-rate=90000 \
             ! webrtcbin name=peer",
        )
        .unwrap()
        .downcast::<gst::Pipeline>()
        .unwrap();
        let webrtc = pipeline.by_name("peer").unwrap();
        pipeline.set_state(gst::State::Ready).unwrap();
        let session = Session {
            pipeline,
            webrtc,
            incoming: Some(Mutex::default()),
        };
        let offer = "v=0\r\no=- 0 0 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n\
            m=video 9 UDP/TLS/RTP/SAVPF 96\r\nc=IN IP4 0.0.0.0\r\n\
            a=mid:0\r\na=recvonly\r\na=rtcp-mux\r\na=setup:actpass\r\n\
            a=ice-ufrag:test\r\na=ice-pwd:testpasswordforthevideooffer\r\n\
            a=fingerprint:sha-256 00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00\r\n\
            a=rtpmap:96 H264/90000\r\n\
            a=fmtp:96 packetization-mode=1;profile-level-id=42e01f\r\n";
        // A silent local STUN endpoint keeps gathering active deterministically.
        let silent_stun = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = silent_stun.local_addr().unwrap().port();
        session
            .webrtc
            .set_property("stun-server", format!("stun://127.0.0.1:{port}"));
        let answer_result =
            tokio::time::timeout(std::time::Duration::from_secs(2), session.answer(offer)).await;
        let answer_result = answer_result.expect("the answer must not wait for STUN gathering");
        let answer = answer_result.unwrap();
        let sdp = gst_sdp::SDPMessage::parse_buffer(answer.as_bytes()).unwrap();
        let video = sdp.media(0).unwrap();
        assert_ne!(video.port(), 0, "the H.264 video offer must be accepted");
        assert_eq!(video.formats().collect::<Vec<_>>(), ["96"]);
        assert!(video.attributes().any(|a| a.key() == "sendonly"));
        assert_eq!(session.pipeline.current_state(), gst::State::Playing);
        assert_ne!(
            session
                .webrtc
                .property::<gst_webrtc::WebRTCICEGatheringState>("ice-gathering-state"),
            gst_webrtc::WebRTCICEGatheringState::Complete
        );
        assert!(
            !answer.contains("a=candidate:"),
            "trickle returns the original SDP, not a gathered snapshot"
        );
        let host = Candidate::Candidate {
            candidate: "candidate:1 1 UDP 1 192.0.2.1 5000 typ host".into(),
            sdp_m_line_index: 0,
        };
        session.candidate(&host).unwrap();
        session.candidate(&Candidate::Complete).unwrap();
        assert!(session.candidate(&Candidate::Complete).is_err());
    }
}

#[cfg(test)]
#[path = "video_tests.rs"]
mod video_tests;
