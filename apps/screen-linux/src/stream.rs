//! Video: a display's capture source (PipeWire or X11, see `backend.rs`) encoded as H.264
//! and sent over GStreamer `webrtcbin` with negotiated trickle ICE, plus one-off JPEG stills.

use crate::backend::{Backend, Display};
use crate::negotiation::ice_uri;
use crate::video::{h264_format, h264_payload};
use crate::{Helper, input};
use anyhow::{Context, Result, anyhow, bail};
use base64::Engine as _;
use gst::prelude::*;
use serde_json::{Value, json};
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

pub struct Session {
    pub(super) pipeline: gst::Pipeline,
    pub(super) webrtc: gst::Element,
    /// The phone's candidates so far; `None` for a complete-SDP session.
    pub(super) incoming: Option<Mutex<crate::candidates::Candidates>>,
    /// Applies the viewer's messages (`spawn_messages`); None in a bare session.
    pub(super) task: Option<tokio::task::JoinHandle<()>>,
}

impl Session {
    /// A session around `pipeline` and `webrtc` with nothing applying messages, for tests.
    #[cfg(test)]
    pub(crate) fn bare(
        pipeline: gst::Pipeline,
        webrtc: gst::Element,
        incoming: Option<Mutex<crate::candidates::Candidates>>,
    ) -> Self {
        Self {
            pipeline,
            webrtc,
            incoming,
            task: None,
        }
    }

    pub fn new(
        id: String,
        display: &Display,
        src: &gst::Element,
        helper: &Arc<Helper>,
        config: &Value,
    ) -> Result<Self> {
        let payload = h264_payload(config["sdp"].as_str().context("missing offer")?)?;
        let pipeline = gst::Pipeline::new();
        // Capture and keepalive timestamps use the system clock. pipewiresrc only gained a
        // switch for this (`provide-clock`) in PipeWire 1.6; Ubuntu 24.04 ships 1.0.
        pipeline.use_clock(Some(&gst::SystemClock::obtain()));
        let convert = make("videoconvert")?;
        let crop = make("videocrop")?;
        let scale = make("videoscale")?;
        let size = make("capsfilter")?;
        let rate = make("videorate")?;
        let fps = config["maxFramerate"].as_i64().unwrap_or(60).clamp(1, 60);
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
        let pay = make("rtph264pay")?;
        pay.set_property("config-interval", -1i32);
        pay.set_property_from_str("aggregate-mode", "zero-latency");
        pay.set_property("pt", u32::try_from(payload)?);
        let rtp_caps = make("capsfilter")?;
        rtp_caps.set_property(
            "caps",
            gst::Caps::builder("application/x-rtp")
                .field("media", "video")
                .field("encoding-name", "H264")
                .field("payload", payload)
                .field("clock-rate", 90000i32)
                .build(),
        );
        let webrtc = make("webrtcbin")?;
        webrtc.set_property_from_str("bundle-policy", "max-bundle");
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
            src, &convert, &crop, &scale, &size, &rate, &convert2, &format, &queue, &encoder,
            &parse, &pay, &rtp_caps, &webrtc,
        ];
        pipeline.add_many(chain)?;
        gst::Element::link_many(chain)?;

        // Input from the phone, applied in order.
        let (tx, rx) = mpsc::unbounded_channel::<Incoming>();
        observe_frame(&convert, tx.downgrade())?;
        let view = View {
            // Weak: the pipeline holds this task's sender, through webrtcbin's data channels.
            pipeline: pipeline.downgrade(),
            src: src.clone(),
            convert: convert.clone(),
            crop: crop.clone(),
            size: size.clone(),
            display: display.clone(),
            frame: None,
            shown: None,
            fit: None,
        };
        let channels: Arc<Mutex<Vec<gst_webrtc::WebRTCDataChannel>>> = Arc::default();
        let task = spawn_messages(helper.clone(), id.clone(), view, channels.clone(), rx);
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
                        let _ = tx.send(Incoming::Viewer(msg));
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
            task: Some(task),
        })
    }

    pub fn close(self) {
        drop(self);
    }
}

/// Applies the viewer's messages in order. Its data channels' handlers hold its sender and it
/// holds the channels, so it never ends by itself: it stops with its session, letting go of them
/// and of the capture source.
fn spawn_messages(
    helper: Arc<Helper>,
    owner: String,
    mut view: View,
    channels: Arc<Mutex<Vec<gst_webrtc::WebRTCDataChannel>>>,
    mut rx: mpsc::UnboundedReceiver<Incoming>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(incoming) = rx.recv().await {
            let msg = match incoming {
                Incoming::Frame(frame) => {
                    view.frame = Some(frame);
                    view.resize();
                    continue;
                }
                Incoming::Viewer(msg) => msg,
            };
            let reply = if msg["type"] == "view" {
                Some(
                    view.view(&helper, &msg)
                        .await
                        .unwrap_or_else(|e| error_msg(&e)),
                )
            } else {
                apply(&helper, &view.display, &owner, &msg)
                    .await
                    .err()
                    .as_ref()
                    .map(error_msg)
            };
            if let Some(reply) = reply {
                send(&channels, &reply);
            }
        }
    })
}

impl Drop for Session {
    fn drop(&mut self) {
        // Negotiation failure must stop capture too, before the session enters the helper's map.
        let _ = self.pipeline.set_state(gst::State::Null);
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

/// What the session's task handles, in order.
enum Incoming {
    /// A message from the viewer.
    Viewer(Value),
    /// The capture source's frame size, sent whenever the source (re)negotiates.
    Frame((f64, f64)),
}

/// Reports each frame size the capture source negotiates into `convert`. Weak: `View` holds
/// `convert`, so a strong sender here would keep the session's task alive forever.
fn observe_frame(convert: &gst::Element, tx: mpsc::WeakUnboundedSender<Incoming>) -> Result<()> {
    let pad = convert
        .static_pad("sink")
        .context("videoconvert has no sink")?;
    pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_, info| {
        if let Some(gst::PadProbeData::Event(event)) = &info.data
            && let gst::EventView::Caps(caps) = event.view()
            && let Some(s) = caps.caps().structure(0)
            && let (Ok(w), Ok(h)) = (s.get::<i32>("width"), s.get::<i32>("height"))
            && let Some(tx) = tx.upgrade()
        {
            let _ = tx.send(Incoming::Frame((f64::from(w), f64::from(h))));
        }
        gst::PadProbeReturn::Ok
    });
    Ok(())
}

/// The display a viewer sees and controls, and the crop and output size it's shown at (see the
/// iOS `ScreenViewport`).
struct View {
    pipeline: gst::glib::WeakRef<gst::Pipeline>,
    /// The display's capture source, feeding `convert`.
    src: gst::Element,
    convert: gst::Element,
    crop: gst::Element,
    size: gst::Element,
    display: Display,
    /// The source's frame size, larger than the display's logical size on HiDPI; `None` until
    /// the current source negotiates.
    frame: Option<(f64, f64)>,
    /// The display region shown (x, y, width, height in logical points); `None` for all of it.
    shown: Option<(f64, f64, f64, f64)>,
    /// The longest and shortest side the viewer shows the picture at, once it said.
    fit: Option<(f64, f64)>,
}

impl View {
    /// A `view` message: moves to the display it names, else takes its crop; its size, when given, either way.
    async fn view(&mut self, helper: &Helper, msg: &Value) -> Result<Value> {
        if let (Some(long), Some(short)) = (msg["long"].as_f64(), msg["short"].as_f64()) {
            self.fit = Some((long, short));
        }
        match msg["display"]
            .as_u64()
            .filter(|&id| id != u64::from(self.display.id))
        {
            // Shown whole until the viewer crops the new display's picture.
            Some(id) => self.switch(helper, id).await?,
            None => {
                self.shown = msg["crop"].as_array().and_then(|c| {
                    let v: Vec<f64> = c.iter().filter_map(Value::as_f64).collect();
                    (v.len() == 4 && v[2] >= 16.0 && v[3] >= 16.0)
                        .then(|| (v[0].max(0.0), v[1].max(0.0), v[2], v[3]))
                });
            }
        }
        self.resize();
        Ok(json!({
            "type": "view",
            "display": self.display.id,
            "crop": self.shown.map(|(x, y, w, h)| json!([x, y, w, h])),
        }))
    }

    /// Moves capture and input to display `id`, shown whole.
    async fn switch(&mut self, helper: &Helper, id: u64) -> Result<()> {
        let backend = helper.backend().await?;
        let display = backend.display(Some(id))?;
        let src = backend.source(&display).await?;
        let pipeline = self
            .pipeline
            .upgrade()
            .context("the screen session ended")?;
        replace_source(&pipeline, &self.src, &src, &self.convert)?;
        self.src = src;
        self.display = display;
        self.frame = None;
        self.shown = None;
        Ok(())
    }

    /// Sets the crop and output size for the shown region, the frame size and the viewer's size.
    fn resize(&self) {
        let (dw, dh) = (self.display.width, self.display.height);
        let (fw, fh) = self.frame.unwrap_or((dw, dh));
        let factor = fw / dw;
        let (x, y, w, h) = self.shown.unwrap_or((0.0, 0.0, dw, dh));
        let px = |v: f64| to_i32(v * factor);
        self.crop.set_property("left", px(x));
        self.crop.set_property("top", px(y));
        self.crop
            .set_property("right", (to_i32(fw) - px(x + w)).max(0));
        self.crop
            .set_property("bottom", (to_i32(fh) - px(y + h)).max(0));
        if let Some((long, short)) = self.fit {
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
    }
}

/// Swaps `old` for `new` in front of `convert`; on failure the old source is put back.
fn replace_source(
    pipeline: &gst::Pipeline,
    old: &gst::Element,
    new: &gst::Element,
    convert: &gst::Element,
) -> Result<()> {
    let attach = |src: &gst::Element| -> Result<()> {
        pipeline.add(src)?;
        src.link(convert)?;
        src.sync_state_with_parent()?;
        Ok(())
    };
    // Stopped first, so the old source never pushes into an unlinked pad.
    old.set_state(gst::State::Null)?;
    // A source a failed earlier swap couldn't put back is already out.
    if old.parent().is_some() {
        pipeline.remove(old)?;
    }
    let Err(error) = attach(new) else {
        return Ok(());
    };
    let _ = new.set_state(gst::State::Null);
    if new.parent().is_some() {
        pipeline.remove(new)?;
    }
    attach(old).with_context(|| format!("{error:#}; couldn't go back to the previous display"))?;
    Err(error)
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "screen sizes are a few thousand pixels"
)]
fn to_i32(v: f64) -> i32 {
    v.round() as i32
}

/// A clipboard or input message from `owner`'s viewer, on the current backend.
async fn apply(helper: &Helper, display: &Display, owner: &str, msg: &Value) -> Result<()> {
    let backend = helper.backend().await?;
    if msg["type"] == "clipboard" {
        return set_clipboard(&backend, msg["text"].as_str().unwrap_or_default()).await;
    }
    helper
        .input(&backend, display, Some(owner), input::Event::parse(msg)?)
        .await
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
async fn set_clipboard(backend: &Backend, text: &str) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    for (cmd, args) in [
        ("wl-copy", &[][..]),
        ("xclip", &["-selection", "clipboard"][..]),
    ] {
        let Ok(mut child) = backend
            .command(cmd)
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
            e.set_property("key-int-max", 120u32);
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
pub async fn screenshot(src: gst::Element, width: u32, height: u32) -> Result<String> {
    let still = gst::parse::bin_from_description(
        &format!(
            "videoconvert ! videoscale ! video/x-raw,width={width},height={height} ! jpegenc quality=80 ! appsink name=sink max-buffers=1"
        ),
        true,
    )?;
    let pipeline = gst::Pipeline::new();
    pipeline.add_many([&src, still.upcast_ref()])?;
    src.link(&still)?;
    let sink = still
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
    async fn a_dropped_session_stops_its_message_task() {
        gst::init().unwrap();
        let held = Arc::new(());
        let mut session = Session::bare(
            gst::Pipeline::new(),
            gst::ElementFactory::make("webrtcbin").build().unwrap(),
            None,
        );
        let guard = held.clone();
        session.task = Some(tokio::spawn(async move {
            std::future::pending::<()>().await;
            drop(guard);
        }));
        drop(session);
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while Arc::strong_count(&held) > 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the task lets go of what it holds");
    }

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
        let session = Session::bare(pipeline, webrtc, Some(Mutex::default()));
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
