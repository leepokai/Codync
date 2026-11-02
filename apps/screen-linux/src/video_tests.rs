#![allow(clippy::unwrap_used)]

use super::*;

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
    assert!(h264_payload(&offer.replace("packetization-mode=1", "packetization-mode=0")).is_err());
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
    let session = Session {
        pipeline,
        webrtc,
        incoming: None,
    };
    let offer = "v=0\r\no=- 0 0 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n\
            m=video 9 UDP/TLS/RTP/SAVPF 102\r\nc=IN IP4 0.0.0.0\r\n\
            a=mid:0\r\na=recvonly\r\na=rtcp-mux\r\na=setup:actpass\r\n\
            a=ice-ufrag:test\r\na=ice-pwd:testpasswordforthevideooffer\r\n\
            a=fingerprint:sha-256 00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00\r\n\
            a=rtpmap:102 H264/90000\r\n\
            a=fmtp:102 packetization-mode=1;profile-level-id=42e01f\r\n";
    let answer_result =
        tokio::time::timeout(std::time::Duration::from_secs(12), session.answer(offer)).await;
    let answer = answer_result
        .expect("complete SDP must finish gathering")
        .unwrap();
    let sdp = gst_sdp::SDPMessage::parse_buffer(answer.as_bytes()).unwrap();
    let video = sdp.media(0).unwrap();
    assert_ne!(video.port(), 0, "the H.264 video offer must be accepted");
    assert_eq!(video.formats().collect::<Vec<_>>(), ["102"]);
    assert!(video.attributes().any(|a| a.key() == "sendonly"));
    assert_eq!(session.pipeline.current_state(), gst::State::Playing);
    assert_eq!(
        session
            .webrtc
            .property::<gst_webrtc::WebRTCICEGatheringState>("ice-gathering-state"),
        gst_webrtc::WebRTCICEGatheringState::Complete,
    );
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
fn capture_source_builds_on_the_installed_pipewire() {
    gst::init().unwrap();
    let fd = std::fs::File::open("/dev/null").unwrap();
    capture_source(fd.into(), 0).unwrap();
}

#[tokio::test]
async fn answer_accepts_the_phones_retransmissions() {
    gst::init().unwrap();
    let pipeline = gst::parse::launch(
        "videotestsrc is-live=true ! videoconvert ! video/x-raw,format=I420 \
             ! x264enc tune=zerolatency ! h264parse name=parse",
    )
    .unwrap()
    .downcast::<gst::Pipeline>()
    .unwrap();
    let [pay, caps] = payloader(102).unwrap();
    let webrtc = sender().unwrap();
    pipeline.add_many([&pay, &caps, &webrtc]).unwrap();
    gst::Element::link_many([&pipeline.by_name("parse").unwrap(), &pay, &caps, &webrtc]).unwrap();
    pipeline.set_state(gst::State::Ready).unwrap();
    let session = Session {
        pipeline,
        webrtc,
        incoming: Some(Mutex::default()),
    };
    // What libwebrtc (iOS) offers for H.264 with retransmissions.
    let offer = "v=0\r\no=- 0 0 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n\
            m=video 9 UDP/TLS/RTP/SAVPF 102 103\r\nc=IN IP4 0.0.0.0\r\n\
            a=mid:0\r\na=recvonly\r\na=rtcp-mux\r\na=setup:actpass\r\n\
            a=ice-ufrag:test\r\na=ice-pwd:testpasswordforthevideooffer\r\n\
            a=fingerprint:sha-256 00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00\r\n\
            a=rtpmap:102 H264/90000\r\na=rtcp-fb:102 nack\r\na=rtcp-fb:102 nack pli\r\n\
            a=fmtp:102 packetization-mode=1;profile-level-id=42e01f\r\n\
            a=rtpmap:103 rtx/90000\r\na=fmtp:103 apt=102\r\n";
    let answer = session.answer(offer).await.unwrap();
    assert!(answer.contains("a=rtpmap:103 rtx/90000"), "{answer}");
    assert!(answer.contains("a=fmtp:103 apt=102"), "{answer}");
    // The phone binds the video to the group's first SSRC: it must be a declared stream, not 0.
    let fid: Vec<&str> = answer
        .lines()
        .find_map(|l| l.strip_prefix("a=ssrc-group:FID "))
        .expect("RTX is grouped with the video")
        .split(' ')
        .collect();
    assert_eq!(fid.len(), 2, "{answer}");
    assert_ne!(fid[0], "0", "{answer}");
    for ssrc in fid {
        assert!(answer.contains(&format!("a=ssrc:{ssrc} msid:")), "{answer}");
    }
}

/// The MTU of the WireGuard tunnels a direct path may cross.
const TUNNEL_MTU: usize = 1280;

/// Encoded frames from `source`, in order: (size, keyframe). `None` when the encoder picked here
/// isn't x264: the keyframe interval is an x264 setting, and hardware encoders keep their own.
fn encoded(source: &str) -> Option<Vec<(usize, bool)>> {
    let pipeline = gst::parse::launch(&format!(
        "{source} ! videoconvert ! video/x-raw,format=I420 ! identity name=raw"
    ))
    .unwrap()
    .downcast::<gst::Pipeline>()
    .unwrap();
    let encoder = encoder(1000).unwrap();
    let name = encoder.factory().map(|f| f.name().to_string());
    if name.as_deref() != Some("x264enc") {
        eprintln!("skipped: the encoder here is {name:?}, not x264enc");
        return None;
    }
    let sink = gst_app::AppSink::builder().sync(false).build();
    pipeline.add_many([&encoder, sink.upcast_ref()]).unwrap();
    gst::Element::link_many([
        &pipeline.by_name("raw").unwrap(),
        &encoder,
        sink.upcast_ref(),
    ])
    .unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();
    let mut frames = Vec::new();
    while let Some(sample) = sink.try_pull_sample(gst::ClockTime::from_seconds(10)) {
        let buffer = sample.buffer().unwrap();
        frames.push((
            buffer.size(),
            !buffer.flags().contains(gst::BufferFlags::DELTA_UNIT),
        ));
    }
    pipeline.set_state(gst::State::Null).unwrap();
    Some(frames)
}

#[test]
fn a_moving_screen_gets_only_safety_net_keyframes() {
    gst::init().unwrap();
    // One interval and a second more at the direct path's 60 fps: the old 120-frame interval made
    // five more keyframes; the safety net adds one at the interval. The ball pattern has no
    // scene cut, so x264 adds none of its own.
    let interval = usize::try_from(KEYFRAME_INTERVAL_FRAMES).unwrap();
    let total = interval + 60;
    let Some(frames) = encoded(&format!(
        "videotestsrc num-buffers={total} pattern=ball ! video/x-raw,width=320,height=240,framerate=60/1"
    )) else {
        return;
    };
    assert_eq!(frames.len(), total, "the pipeline must encode every frame");
    let keyframes: Vec<_> = (0..frames.len()).filter(|&i| frames[i].1).collect();
    assert_eq!(keyframes, [0, interval]);
}

#[test]
fn rtp_packets_fit_a_wireguard_tunnel_without_fragmenting() {
    gst::init().unwrap();
    let pipeline = gst::parse::launch(
        "videotestsrc num-buffers=2 pattern=snow ! video/x-raw,width=1280,height=720 \
             ! videoconvert ! video/x-raw,format=I420 ! x264enc tune=zerolatency \
             ! h264parse name=parse",
    )
    .unwrap()
    .downcast::<gst::Pipeline>()
    .unwrap();
    let [pay, caps] = payloader(102).unwrap();
    let sink = gst_app::AppSink::builder().sync(false).build();
    pipeline.add_many([&pay, &caps, sink.upcast_ref()]).unwrap();
    gst::Element::link_many([
        &pipeline.by_name("parse").unwrap(),
        &pay,
        &caps,
        sink.upcast_ref(),
    ])
    .unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();
    let mut sizes = Vec::new();
    let mut ssrcs = std::collections::BTreeSet::new();
    while let Some(sample) = sink.try_pull_sample(gst::ClockTime::from_seconds(10)) {
        let map = sample.buffer().unwrap().map_readable().unwrap();
        sizes.push(map.len());
        ssrcs.insert(u32::from_be_bytes(map[8..12].try_into().unwrap()));
    }
    pipeline.set_state(gst::State::Null).unwrap();
    // The SSRC the answer signals (from these caps) is the one on the wire.
    let signalled = caps
        .property::<gst::Caps>("caps")
        .structure(0)
        .unwrap()
        .get::<u32>("ssrc")
        .unwrap();
    assert_eq!(ssrcs.into_iter().collect::<Vec<_>>(), [signalled]);
    assert!(sizes.len() > 1, "a noisy keyframe spans several packets");
    // On top of each RTP packet: a resend's 2-byte original sequence number (RTX), SRTP's tag
    // (10 bytes, 16 with GCM), UDP 8 and IP 20 (40 over IPv6).
    let largest = sizes.iter().max().unwrap();
    assert!(
        largest + 2 + 16 + 8 + 40 <= TUNNEL_MTU,
        "largest RTP packet: {largest}"
    );
}
