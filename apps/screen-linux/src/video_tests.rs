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
    let session = Session { pipeline, webrtc };
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
