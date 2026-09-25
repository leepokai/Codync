//! Video: a PipeWire monitor stream encoded as H.264 and sent over GStreamer
//! `webrtcbin` (non-trickle, like the macOS helper), plus one-off JPEG stills.

use crate::portal::Display;
use crate::{Helper, input};
use anyhow::{Context, Result, anyhow};
use base64::Engine as _;
use gst::prelude::*;
use serde_json::{Value, json};
use std::os::fd::{IntoRawFd, OwnedFd};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

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

pub struct Session {
    pipeline: gst::Pipeline,
    webrtc: gst::Element,
}

impl Session {
    pub fn new(id: String, display: &Display, fd: OwnedFd, helper: Arc<Helper>) -> Result<Self> {
        let pipeline = gst::Pipeline::new();
        let src = gst::ElementFactory::make("pipewiresrc")
            .property("fd", fd.into_raw_fd())
            .property("path", display.node.to_string())
            .property("do-timestamp", true)
            .build()
            .context("GStreamer's PipeWire plugin (gstreamer1.0-pipewire) is missing")?;
        if src.has_property("keepalive-time") {
            // A still screen keeps sending frames now and then, so keyframe requests get answered.
            src.set_property("keepalive-time", 1000i32);
        }
        let convert = make("videoconvert")?;
        let crop = make("videocrop")?;
        let scale = make("videoscale")?;
        let size = make("capsfilter")?;
        let rate = make("videorate")?;
        rate.set_property("max-rate", 60i32);
        rate.set_property("drop-only", true);
        let convert2 = make("videoconvert")?;
        let queue = make("queue")?;
        queue.set_property_from_str("leaky", "downstream");
        queue.set_property("max-size-buffers", 2u32);
        let encoder = encoder()?;
        let parse = make("h264parse")?;
        let pay = make("rtph264pay")?;
        pay.set_property("config-interval", -1i32);
        pay.set_property_from_str("aggregate-mode", "zero-latency");
        pay.set_property("pt", 96u32);
        let rtp_caps = make("capsfilter")?;
        rtp_caps.set_property(
            "caps",
            gst::Caps::builder("application/x-rtp")
                .field("media", "video")
                .field("encoding-name", "H264")
                .field("payload", 96i32)
                .field("clock-rate", 90000i32)
                .build(),
        );
        let webrtc = make("webrtcbin")?;
        webrtc.set_property_from_str("bundle-policy", "max-bundle");
        let chain = [
            &src, &convert, &crop, &scale, &size, &rate, &convert2, &queue, &encoder, &parse, &pay,
            &rtp_caps, &webrtc,
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
                            set_clipboard(msg["text"].as_str().unwrap_or_default())
                                .await
                                .err()
                                .as_ref()
                                .map(error_msg)
                        }
                        _ => input::perform(&helper.portal, &display, &msg)
                            .await
                            .err()
                            .as_ref()
                            .map(error_msg),
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
        webrtc.connect_notify(Some("ice-connection-state"), move |w, _| {
            let state = w.property::<gst_webrtc::WebRTCICEConnectionState>("ice-connection-state");
            if matches!(
                state,
                gst_webrtc::WebRTCICEConnectionState::Failed
                    | gst_webrtc::WebRTCICEConnectionState::Closed
            ) {
                helper.ended(&id);
            }
        });
        pipeline
            .set_state(gst::State::Playing)
            .context("starting the video pipeline")?;
        Ok(Self { pipeline, webrtc })
    }

    /// Answers an offer (first connect or ICE restart) once every local candidate is in.
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
        let reply = done.await?.ok_or_else(|| anyhow!("no answer"))?;
        let answer = reply
            .get::<gst_webrtc::WebRTCSessionDescription>("answer")
            .map_err(|_| anyhow!("no answer"))?;
        let (p, done) = promise();
        self.webrtc
            .emit_by_name::<()>("set-local-description", &[&answer, &p]);
        done.await?;
        for _ in 0..60 {
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

    pub fn close(self) {
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

/// Wayland first, then X11.
async fn set_clipboard(text: &str) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    for (cmd, args) in [
        ("wl-copy", &[][..]),
        ("xclip", &["-selection", "clipboard"][..]),
    ] {
        let Ok(mut child) = tokio::process::Command::new(cmd)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .spawn()
        else {
            continue;
        };
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes()).await?;
        }
        if child.wait().await?.success() {
            return Ok(());
        }
    }
    Err(anyhow!(
        "Install wl-clipboard (or xclip) on this computer to paste from your phone."
    ))
}

fn make(name: &str) -> Result<gst::Element> {
    gst::ElementFactory::make(name)
        .build()
        .with_context(|| format!("GStreamer element {name} is missing"))
}

fn encoder() -> Result<gst::Element> {
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
            e.set_property("bitrate", BITRATE_KBPS);
            e.set_property("key-int-max", 120u32);
        }
        "openh264enc" => e.set_property("bitrate", BITRATE_KBPS * 1000),
        _ => {
            if e.has_property("bitrate") {
                e.set_property_from_str("bitrate", &BITRATE_KBPS.to_string());
            }
        }
    }
    tracing::info!(encoder = name, "video encoder");
    Ok(e)
}

fn promise() -> (gst::Promise, oneshot::Receiver<Option<gst::Structure>>) {
    let (tx, rx) = oneshot::channel();
    let p = gst::Promise::with_change_func(move |reply| {
        let _ = tx.send(reply.ok().flatten().map(ToOwned::to_owned));
    });
    (p, rx)
}

/// One still for agents: `width`×`height` JPEG, base64.
pub async fn screenshot(fd: OwnedFd, d: &Display, width: u32, height: u32) -> Result<String> {
    let desc = format!(
        "pipewiresrc name=src path={} do-timestamp=true ! videoconvert ! videoscale ! video/x-raw,width={width},height={height} ! jpegenc quality=80 ! appsink name=sink max-buffers=1",
        d.node
    );
    let pipeline = gst::parse::launch(&desc)?
        .downcast::<gst::Pipeline>()
        .map_err(|_| anyhow!("bad pipeline"))?;
    let src = pipeline
        .by_name("src")
        .ok_or_else(|| anyhow!("no source"))?;
    src.set_property("fd", fd.into_raw_fd());
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
