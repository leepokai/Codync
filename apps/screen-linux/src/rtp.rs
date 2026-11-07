//! The RTP side of a screen stream: H.264 packets sized for tunnels with the SSRC signalled up
//! front, and a `webrtcbin` that resends lost packets.

use crate::stream::make;
use anyhow::Result;
use gst::prelude::*;

/// RTP packet size, RTP header included: libwebrtc's size.
const RTP_MTU: u32 = 1200;

/// H.264 RTP packets small enough for a tunnel, and the caps `webrtcbin` answers from.
pub fn payloader(payload: i32) -> Result<[gst::Element; 2]> {
    let pay = make("rtph264pay")?;
    // The default 1400 plus SRTP/UDP/IP is over a WireGuard tunnel's 1280-byte MTU, so a full
    // packet had to leave as two IP fragments, and losing either loses it.
    pay.set_property("mtu", RTP_MTU);
    pay.set_property("config-interval", -1i32);
    pay.set_property_from_str("aggregate-mode", "zero-latency");
    pay.set_property("pt", u32::try_from(payload)?);
    // The answer is written before any packet flows. Without the SSRC in the caps, adding RTX
    // signals `ssrc-group:FID 0 <rtx>` and only the RTX SSRC, so the phone (iOS WebRTC 153)
    // waits for video on the RTX stream and shows black. Brave tolerates it.
    let ssrc = random_ssrc();
    pay.set_property("ssrc", ssrc);
    let caps = make("capsfilter")?;
    caps.set_property(
        "caps",
        gst::Caps::builder("application/x-rtp")
            .field("media", "video")
            .field("encoding-name", "H264")
            .field("payload", payload)
            .field("clock-rate", 90000i32)
            .field("ssrc", ssrc)
            .build(),
    );
    Ok([pay, caps])
}

/// Never 0: an SSRC of 0 is exactly what the phone couldn't bind to.
fn random_ssrc() -> u32 {
    gst::glib::random_int().max(1)
}

/// `webrtcbin` that resends lost packets.
pub fn sender() -> Result<gst::Element> {
    let webrtc = make("webrtcbin")?;
    webrtc.set_property_from_str("bundle-policy", "max-bundle");
    // Linking the payloader requests the sink pad, which creates the transceiver. With NACK the
    // answer also accepts the phone's RTX, so a lost packet is resent instead of costing a keyframe.
    webrtc.connect("on-new-transceiver", false, |values| {
        if let Ok(transceiver) = values[1].get::<gst_webrtc::WebRTCRTPTransceiver>() {
            transceiver.set_property("do-nack", true);
        }
        None
    });
    Ok(webrtc)
}
