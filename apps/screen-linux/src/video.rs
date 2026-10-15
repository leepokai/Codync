//! Linux capture format and offered H.264 payload selection.

use anyhow::{Result, anyhow, bail};

/// The answer must use the phone's payload number, which is not necessarily 96.
pub(super) fn h264_payload(offer: &str) -> Result<i32> {
    let sdp =
        gst_sdp::SDPMessage::parse_buffer(offer.as_bytes()).map_err(|_| anyhow!("bad offer"))?;
    for media in sdp.medias().filter(|m| m.media() == Some("video")) {
        for format in media.formats() {
            // rtph264pay only takes dynamic payload numbers.
            let Ok(payload @ 96..=127) = format.parse::<i32>() else {
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

/// WebRTC Baseline requires 8-bit 4:2:0; RGB capture must not select 4:4:4.
pub(super) fn h264_format() -> Result<gst::Element> {
    Ok(gst::ElementFactory::make("capsfilter")
        .property(
            "caps",
            gst::Caps::builder("video/x-raw")
                .field("format", gst::List::new(["I420", "NV12"]))
                .build(),
        )
        .build()?)
}
