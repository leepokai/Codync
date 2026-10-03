//! Video: a PipeWire monitor stream encoded as H.264 and sent over GStreamer
//! `webrtcbin` (non-trickle, like the macOS helper), plus one-off JPEG stills.

use crate::portal::Display;
use crate::{Helper, input};
use anyhow::{Context, Result, anyhow, bail};
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
    pub fn new(
        id: String,
        display: &Display,
        fd: OwnedFd,
        helper: Arc<Helper>,
        config: &Value,
    ) -> Result<Self> {
        let payload = h264_payload(config["sdp"].as_str().context("missing offer")?)?;
        let pipeline = gst::Pipeline::new();
        let src = gst::ElementFactory::make("pipewiresrc")
            .property("fd", fd.into_raw_fd())
            .property("path", display.node.to_string())
            .property("do-timestamp", true)
            // PipeWire's stream clock can have a different origin from capture and
            // keepalive timestamps, making the source wait on stale/future frames.
            .property("provide-clock", false)
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
        let runtime = tokio::runtime::Handle::current();
        webrtc.connect_notify(Some("ice-connection-state"), move |w, _| {
            let state = w.property::<gst_webrtc::WebRTCICEConnectionState>("ice-connection-state");
            if matches!(
                state,
                gst_webrtc::WebRTCICEConnectionState::Failed
                    | gst_webrtc::WebRTCICEConnectionState::Closed
            ) {
                // GStreamer signals run on its own threads, outside Tokio.
                let _guard = runtime.enter();
                helper.ended(&id);
            }
        });
        pipeline
            .set_state(gst::State::Ready)
            .context("preparing the video pipeline")?;
        Ok(Self { pipeline, webrtc })
    }

    /// Answers once a relay route is available or gathering has completed.
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
        // Negotiate before the encoder chooses fixed caps. Starting first can
        // produce High-profile caps that reject a phone's Baseline offer.
        self.pipeline
            .set_state(gst::State::Playing)
            .context("starting the video pipeline")?;
        for _ in 0..200 {
            let local = self
                .webrtc
                .property::<Option<gst_webrtc::WebRTCSessionDescription>>("local-description");
            if local.as_ref().is_some_and(|s| has_relay_candidate(s.sdp()))
                || self
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

/// One relay candidate is enough for a non-trickle answer; later duplicate
/// TURN transports must not delay a route that can already carry the session.
fn has_relay_candidate(sdp: &gst_sdp::SDPMessageRef) -> bool {
    sdp.medias().any(|media| {
        media.attributes().any(|attribute| {
            attribute.key() == "candidate"
                && attribute.value().is_some_and(|candidate| {
                    candidate
                        .split_ascii_whitespace()
                        .collect::<Vec<_>>()
                        .windows(2)
                        .any(|pair| pair == ["typ", "relay"])
                })
        })
    })
}

/// The answer must use the phone's payload number, which is not necessarily 96.
fn h264_payload(offer: &str) -> Result<i32> {
    let sdp =
        gst_sdp::SDPMessage::parse_buffer(offer.as_bytes()).map_err(|_| anyhow!("bad offer"))?;
    for media in sdp.medias().filter(|m| m.media() == Some("video")) {
        for format in media.formats() {
            let Ok(payload @ 0..=127) = format.parse::<i32>() else {
                continue;
            };
            let Some(caps) = media.caps_from_media(payload) else {
                continue;
            };
            let Some(codec) = caps.structure(0) else {
                continue;
            };
            if codec.get::<&str>("encoding-name").ok() == Some("H264")
                && codec.get::<i32>("clock-rate").ok() == Some(90_000)
                && codec.get::<&str>("packetization-mode").ok() == Some("1")
            {
                return Ok(payload);
            }
        }
    }
    bail!("the phone did not offer packetized H.264 video")
}

/// Convert WebRTC's ICE URL syntax to GStreamer's URI syntax without exposing credentials.
fn ice_uri(url: &str, username: Option<&str>, credential: Option<&str>) -> Result<String> {
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

/// WebRTC Baseline requires 8-bit 4:2:0; RGB capture must not select 4:4:4.
fn h264_format() -> Result<gst::Element> {
    Ok(gst::ElementFactory::make("capsfilter")
        .property(
            "caps",
            gst::Caps::builder("video/x-raw")
                .field("format", gst::List::new(["I420", "NV12"]))
                .build(),
        )
        .build()?)
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn relay_answer_does_not_wait_for_duplicate_turn_transports() {
        gst::init().unwrap();
        let offer = "v=0\r\no=- 0 0 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n\
            m=video 9 UDP/TLS/RTP/SAVPF 102\r\n\
            a=candidate:1 1 UDP 2130706431 192.0.2.1 5000 typ host\r\n";
        let host_only = gst_sdp::SDPMessage::parse_buffer(offer.as_bytes()).unwrap();
        assert!(!has_relay_candidate(&host_only));
        let relay = format!(
            "{offer}a=candidate:2 1 UDP 16777215 198.51.100.1 6000 typ relay raddr 192.0.2.1 rport 5000\r\n"
        );
        let relay = gst_sdp::SDPMessage::parse_buffer(relay.as_bytes()).unwrap();
        assert!(has_relay_candidate(&relay));
    }

    #[test]
    fn video_uses_the_offered_h264_payload_instead_of_a_fixed_number() {
        gst::init().unwrap();
        let offer = "v=0\r\no=- 0 0 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n\
            m=audio 9 UDP/TLS/RTP/SAVPF 99\r\na=rtpmap:99 H264/90000\r\n\
            a=fmtp:99 packetization-mode=1\r\n\
            m=video 9 UDP/TLS/RTP/SAVPF 96 100 102\r\n\
            a=rtpmap:96 VP8/90000\r\na=rtpmap:100 H264/90000\r\n\
            a=fmtp:100 packetization-mode=0\r\na=rtpmap:102 H264/90000\r\n\
            a=fmtp:102 packetization-mode=1;profile-level-id=42e01f\r\n";
        assert_eq!(h264_payload(offer).unwrap(), 102);
        assert!(
            h264_payload(&offer.replace("packetization-mode=1", "packetization-mode=0")).is_err()
        );
    }

    #[tokio::test]
    async fn baseline_offer_starts_video_after_the_answer_is_negotiated() {
        gst::init().unwrap();
        let pipeline = gst::parse::launch(
            "videotestsrc is-live=true ! videoconvert ! video/x-raw,format=I420 \
             ! x264enc tune=zerolatency ! h264parse ! rtph264pay pt=102 \
             ! application/x-rtp,media=video,encoding-name=H264,payload=102,clock-rate=90000 \
             ! webrtcbin name=peer",
        )
        .unwrap()
        .downcast::<gst::Pipeline>()
        .unwrap();
        let webrtc = pipeline.by_name("peer").unwrap();
        pipeline.set_state(gst::State::Ready).unwrap();
        let session = Session { pipeline, webrtc };
        let offer = "v=0\r\no=- 0 0 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n\
            m=video 9 UDP/TLS/RTP/SAVPF 102\r\nc=IN IP4 0.0.0.0\r\n\
            a=mid:0\r\na=recvonly\r\na=rtcp-mux\r\na=setup:actpass\r\n\
            a=ice-ufrag:test\r\na=ice-pwd:testpasswordforthevideooffer\r\n\
            a=fingerprint:sha-256 00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00\r\n\
            a=rtpmap:102 H264/90000\r\n\
            a=fmtp:102 packetization-mode=1;profile-level-id=42e01f\r\n";
        let answer_result = session.answer(offer).await;
        let answer = answer_result.unwrap();
        let sdp = gst_sdp::SDPMessage::parse_buffer(answer.as_bytes()).unwrap();
        let video = sdp.media(0).unwrap();
        assert_ne!(video.port(), 0, "the H.264 video offer must be accepted");
        assert_eq!(video.formats().collect::<Vec<_>>(), ["102"]);
        assert!(video.attributes().any(|a| a.key() == "sendonly"));
        assert_eq!(session.pipeline.current_state(), gst::State::Playing);
    }

    #[test]
    fn capture_encodes_as_eight_bit_420_before_webrtc_negotiates() {
        gst::init().unwrap();
        let pipeline = gst::Pipeline::new();
        let source = gst::ElementFactory::make("videotestsrc")
            .property("num-buffers", 1i32)
            .build()
            .unwrap();
        let rgb = gst::ElementFactory::make("capsfilter")
            .property(
                "caps",
                gst::Caps::builder("video/x-raw")
                    .field("format", "RGB")
                    .build(),
            )
            .build()
            .unwrap();
        let convert = make("videoconvert").unwrap();
        let format = h264_format().unwrap();
        let encoder = encoder(1000).unwrap();
        let parse = make("h264parse").unwrap();
        let sink = gst_app::AppSink::builder().build();
        let chain = [
            &source,
            &rgb,
            &convert,
            &format,
            &encoder,
            &parse,
            sink.upcast_ref(),
        ];
        pipeline.add_many(chain).unwrap();
        gst::Element::link_many(chain).unwrap();
        pipeline.set_state(gst::State::Playing).unwrap();
        let sample = sink.try_pull_sample(gst::ClockTime::from_seconds(5));
        pipeline.set_state(gst::State::Null).unwrap();
        let sample = sample.expect("RGB capture should produce an H.264 frame");
        let caps = sample.caps().unwrap();
        let h264_caps = caps.structure(0).unwrap();
        assert_eq!(h264_caps.get::<String>("chroma-format").unwrap(), "4:2:0");
        assert_eq!(h264_caps.get::<u32>("bit-depth-luma").unwrap(), 8);
        assert_eq!(h264_caps.get::<u32>("bit-depth-chroma").unwrap(), 8);
    }

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
