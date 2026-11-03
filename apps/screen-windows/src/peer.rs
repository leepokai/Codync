//! One viewer's WebRTC connection: answers the phone's offer, streams the screen, applies its input.

use crate::candidates::{Candidate, Candidates};
use crate::displays::Monitor;
use crate::helper::Helper;
use crate::input::Input;
use crate::stream::{self, Control, Frame};
use crate::view::View;
use anyhow::{Context, Result, anyhow};
use rtc::media::Sample;
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::media_engine::MIME_TYPE_H264;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodecParameters, RTCRtpCodingParameters, RTCRtpEncodingParameters,
    RtpCodecKind,
};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, Weak};
use std::time::Instant;
use tokio::sync::mpsc;
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_local::static_sample::TrackLocalStaticSample;
use webrtc::peer_connection::{
    MediaEngine, PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler,
    RTCConfigurationBuilder, RTCIceCandidateInit, RTCIceGatheringState, RTCIceServer,
    RTCPeerConnectionIceEvent, RTCPeerConnectionState, RTCSessionDescription, Registry,
    register_default_interceptors,
};
use webrtc::runtime::default_runtime;

/// H.264 as phones offer it; the encoder always produces Constrained Baseline, which every decoder accepts.
const H264_PROFILES: &[&str] = &["42e01f", "42001f", "4d001f", "640c1f", "640c28"];
const FIRST_PAYLOAD_TYPE: u8 = 100;
const MAX_FRAMES_QUEUED: usize = 4;

pub struct Session {
    peer: Arc<dyn PeerConnection>,
    control: Arc<Control>,
    /// The phone's candidates so far; `None` for a complete-SDP session.
    incoming: Option<Mutex<Candidates>>,
    id: String,
    helper: Weak<Helper>,
}

/// Everything the events need.
struct Events {
    id: String,
    helper: Weak<Helper>,
    trickle: bool,
    outgoing: Mutex<Candidates>,
    monitor: Monitor,
    control: Arc<Control>,
    connected: mpsc::UnboundedSender<RTCPeerConnectionState>,
    input: mpsc::UnboundedSender<Input>,
    replies: Mutex<Vec<Arc<dyn DataChannel>>>,
    gathered: tokio::sync::Notify,
}

impl Session {
    /// Answers `params` (the host's `answer` request); the phone's media starts once ICE connects.
    pub async fn answer(
        id: String,
        helper: &Arc<Helper>,
        monitor: Monitor,
        params: &Value,
    ) -> Result<(Self, String)> {
        let offer = params["sdp"]
            .as_str()
            .context("sdp is required")?
            .to_owned();
        let trickle = params["trickle"] == true;
        let fps = u32::try_from(params["maxFramerate"].as_i64().unwrap_or(60).clamp(1, 60))?;
        let bitrate = u32::try_from(
            params["maxBitrateBps"]
                .as_u64()
                .unwrap_or(8_000_000)
                .clamp(100_000, 16_000_000),
        )?;

        let mut media = MediaEngine::default();
        for (index, profile) in H264_PROFILES.iter().enumerate() {
            media.register_codec(codec(index, profile), RtpCodecKind::Video)?;
        }
        let registry = register_default_interceptors(Registry::new(), &mut media)?;
        let config = RTCConfigurationBuilder::new()
            .with_ice_servers(ice_servers(&params["iceServers"])?)
            .build();

        let control = Arc::new(Control::default());
        let (connected, mut connection) = mpsc::unbounded_channel();
        let (input_tx, input_rx) = mpsc::unbounded_channel();
        let events = Arc::new(Events {
            id: id.clone(),
            helper: Arc::downgrade(helper),
            trickle,
            outgoing: Mutex::default(),
            monitor: monitor.clone(),
            control: control.clone(),
            connected,
            input: input_tx,
            replies: Mutex::default(),
            gathered: tokio::sync::Notify::new(),
        });
        let peer: Arc<dyn PeerConnection> = Arc::new(
            Box::pin(
                PeerConnectionBuilder::new()
                    .with_configuration(config)
                    .with_media_engine(media)
                    .with_interceptor_registry(registry)
                    .with_handler(Arc::new(Handler(events.clone())))
                    .with_runtime(default_runtime().context("no async runtime")?)
                    .with_udp_addrs(vec!["0.0.0.0:0".to_owned()])
                    .build(),
            )
            .await
            .context("creating the WebRTC connection")?,
        );

        let ssrc = rand_ssrc();
        let track = Arc::new(TrackLocalStaticSample::new(
            Instant::now(),
            MediaStreamTrack::new(
                format!("screen-{id}"),
                "screen-video".into(),
                "Screen".into(),
                RtpCodecKind::Video,
                vec![RTCRtpEncodingParameters {
                    rtp_coding_parameters: RTCRtpCodingParameters {
                        ssrc: Some(ssrc),
                        ..Default::default()
                    },
                    codec: codec(0, H264_PROFILES[0]).rtp_codec,
                    ..Default::default()
                }],
            ),
        )?);
        let sender = peer.add_track(track.clone() as Arc<dyn TrackLocal>).await?;

        peer.set_remote_description(RTCSessionDescription::offer(offer)?)
            .await
            .context("the phone's offer was refused")?;
        let answer = peer.create_answer(None).await?;
        // The original answer carries no candidates; trickle sends them one by one.
        let original = answer.sdp.clone();
        peer.set_local_description(answer).await?;
        let sdp = if trickle {
            original
        } else {
            // Complete-SDP phones need every transport, including late relay candidates.
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(10),
                events.gathered.notified(),
            )
            .await;
            peer.local_description()
                .await
                .map_or(original, |local| local.sdp)
        };

        let payload_type = sender
            .get_parameters()
            .await?
            .rtp_parameters
            .codecs
            .first()
            .map(|c| c.payload_type)
            .ok_or_else(|| anyhow!("the phone did not offer packetized H.264 video"))?;

        // Input from the phone, applied in order on a blocking thread.
        tokio::spawn(apply_input(input_rx, monitor.clone(), events.clone()));
        // Video starts when ICE connects, and the session ends when the connection does.
        let (frames_tx, mut frames_rx) = mpsc::channel::<Frame>(MAX_FRAMES_QUEUED);
        let started = control.clone();
        let watcher = events.clone();
        tokio::spawn(async move {
            let mut video = None;
            while let Some(state) = connection.recv().await {
                match state {
                    RTCPeerConnectionState::Connected if video.is_none() => {
                        video = Some(stream::spawn(
                            watcher.monitor.clone(),
                            fps,
                            bitrate,
                            started.clone(),
                            frames_tx.clone(),
                        ));
                        watcher.notify_session("connected");
                    }
                    RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed => {
                        started.stop.store(true, Ordering::Relaxed);
                        watcher.ended();
                        break;
                    }
                    _ => {}
                }
            }
        });
        tokio::spawn(async move {
            while let Some(frame) = frames_rx.recv().await {
                let sample = Sample {
                    data: frame.data,
                    timestamp: Instant::now(),
                    duration: frame.duration,
                    packet_timestamp: 0,
                    prev_dropped_packets: 0,
                    prev_padding_packets: 0,
                };
                if let Err(error) = track.write_sample(ssrc, payload_type, &sample, &[]).await {
                    tracing::debug!(%error, "couldn't send a frame");
                }
            }
        });

        let session = Self {
            peer,
            control,
            incoming: trickle.then(Mutex::default),
            id,
            helper: Arc::downgrade(helper),
        };
        Ok((session, sdp))
    }

    pub async fn candidate(&self, candidate: &Candidate) -> Result<()> {
        self.incoming
            .as_ref()
            .context("screen session does not support trickle ICE")?
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .accept(candidate)?;
        match candidate {
            Candidate::Complete => {}
            Candidate::Candidate {
                candidate,
                sdp_m_line_index,
                sdp_mid,
            } => {
                let init = RTCIceCandidateInit {
                    candidate: candidate.clone(),
                    sdp_mid: sdp_mid.clone(),
                    sdp_mline_index: Some(u16::try_from(*sdp_m_line_index)?),
                    ..Default::default()
                };
                self.peer
                    .add_ice_candidate(init)
                    .await
                    .context("the phone's ICE candidate was refused")?;
            }
        }
        Ok(())
    }

    pub async fn close(self) {
        self.control.stop.store(true, Ordering::Relaxed);
        if let Err(error) = self.peer.close().await {
            tracing::debug!(%error, session = self.id, "closing the WebRTC connection");
        }
        drop(self.helper);
    }
}

fn codec(index: usize, profile: &str) -> RTCRtpCodecParameters {
    RTCRtpCodecParameters {
        rtp_codec: RTCRtpCodec {
            mime_type: MIME_TYPE_H264.to_owned(),
            clock_rate: 90000,
            channels: 0,
            sdp_fmtp_line: format!(
                "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id={profile}"
            ),
            rtcp_feedback: vec![],
        },
        payload_type: FIRST_PAYLOAD_TYPE + u8::try_from(index).unwrap_or(0),
    }
}

fn rand_ssrc() -> u32 {
    use std::hash::{BuildHasher, RandomState};
    u32::try_from(RandomState::new().hash_one(Instant::now()) & 0xffff_ffff)
        .unwrap_or(1)
        .max(1)
}

/// STUN and UDP TURN servers: `webrtc-rs` relays over UDP only, so TCP and TLS URLs are left out.
fn ice_servers(value: &Value) -> Result<Vec<RTCIceServer>> {
    let mut servers = Vec::new();
    for server in value.as_array().into_iter().flatten() {
        let urls: Vec<String> = server["urls"]
            .as_array()
            .context("missing ICE URLs")?
            .iter()
            .filter_map(Value::as_str)
            .filter(|url| {
                url.starts_with("stun:")
                    || (url.starts_with("turn:") && !url.contains("transport=tcp"))
            })
            .map(str::to_owned)
            .collect();
        if urls.is_empty() {
            continue;
        }
        servers.push(RTCIceServer {
            urls,
            username: server["username"].as_str().unwrap_or_default().to_owned(),
            credential: server["credential"].as_str().unwrap_or_default().to_owned(),
        });
    }
    Ok(servers)
}

async fn apply_input(
    mut input: mpsc::UnboundedReceiver<Input>,
    monitor: Monitor,
    events: Arc<Events>,
) {
    while let Some(event) = input.recv().await {
        let monitor = monitor.clone();
        let result =
            tokio::task::spawn_blocking(move || crate::sendinput::perform(&monitor, &event)).await;
        let failure = match result {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(format!("{error:#}")),
            Err(error) => Some(error.to_string()),
        };
        if let Some(message) = failure {
            events
                .reply(&json!({"type": "error", "message": message}))
                .await;
        }
    }
}

impl Events {
    fn notify(&self, method: &str, params: Value) {
        if let Some(helper) = self.helper.upgrade() {
            let _ = helper.notify(method, params);
        }
    }

    fn notify_session(&self, state: &str) {
        self.notify("session", json!({"session": self.id, "state": state}));
    }

    /// The connection ended on its own: the host drops the session.
    fn ended(&self) {
        let Some(helper) = self.helper.upgrade() else {
            return;
        };
        let id = self.id.clone();
        tokio::spawn(async move { helper.ended(&id).await });
    }

    /// Answers the phone on its `input` channel.
    async fn reply(&self, message: &Value) {
        let channels = self
            .replies
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        for channel in channels {
            if channel.label().await.is_ok_and(|label| label == "input") {
                let _ = channel.send_text(&message.to_string()).await;
            }
        }
    }

    fn send_candidate(&self, candidate: &Candidate) {
        let accepted = self
            .outgoing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .accept(candidate);
        match accepted {
            Ok(()) => self.notify(
                "candidate",
                json!({"session": self.id, "candidate": candidate}),
            ),
            Err(error) => {
                tracing::warn!(
                    error = format!("{error:#}"),
                    "screen candidate exchange failed"
                );
                self.ended();
            }
        }
    }
}

/// Handles one data-channel message from the phone.
async fn handle_message(events: &Events, text: &str) {
    let Ok(message) = serde_json::from_str::<Value>(text) else {
        return;
    };
    if message["type"] == "view" {
        // The video thread applies it; the phone learns what it got from the echo.
        let view = View::apply(&message, events.monitor.width, events.monitor.height);
        *events
            .control
            .view
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(message);
        let crop = view
            .zoomed
            .then(|| json!([view.crop.x, view.crop.y, view.crop.w, view.crop.h]));
        events
            .reply(&json!({"type": "view", "display": events.monitor.id, "crop": crop}))
            .await;
        return;
    }
    match Input::parse(&message) {
        Ok(event) => {
            let _ = events.input.send(event);
        }
        Err(error) => {
            events
                .reply(&json!({"type": "error", "message": format!("{error:#}")}))
                .await;
        }
    }
}

#[derive(Clone)]
struct Handler(Arc<Events>);

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        if !self.0.trickle {
            return;
        }
        let Ok(init) = event.candidate.to_json() else {
            return;
        };
        self.0.send_candidate(&Candidate::Candidate {
            candidate: init.candidate,
            sdp_m_line_index: u32::from(init.sdp_mline_index.unwrap_or(0)),
            sdp_mid: init.sdp_mid,
        });
    }

    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state != RTCIceGatheringState::Complete {
            return;
        }
        self.0.gathered.notify_one();
        if self.0.trickle {
            self.0.send_candidate(&Candidate::Complete);
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        tracing::info!(session = self.0.id, %state, "connection state");
        let _ = self.0.connected.send(state);
    }

    async fn on_data_channel(&self, channel: Arc<dyn DataChannel>) {
        self.0
            .replies
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(channel.clone());
        let events = self.0.clone();
        tokio::spawn(async move {
            while let Some(event) = channel.poll().await {
                match event {
                    DataChannelEvent::OnMessage(message) => {
                        if let Ok(text) = std::str::from_utf8(&message.data) {
                            handle_message(&events, text).await;
                        }
                    }
                    DataChannelEvent::OnClose => break,
                    _ => {}
                }
            }
        });
    }
}
