//! `codync-screen` for Windows: Remote screen for the signed-in desktop.
//! The host (`codync-host`) starts it while Remote screen is on; it connects to the host's
//! named pipe and serves the same JSON-RPC as the macOS and Linux helpers (see
//! `host/src/screen/mod.rs`): DXGI Desktop Duplication for capture, a Media Foundation
//! H.264 encoder, WebRTC (`webrtc-rs`) for the phone, and `SendInput` for its touches and keys.

#![cfg_attr(test, allow(clippy::unwrap_used))]
// Off Windows only the portable modules build, so the protocol code looks unused there.
#![cfg_attr(not(windows), allow(dead_code))]

mod candidates;
#[cfg(windows)]
mod capture;
#[cfg(windows)]
mod displays;
#[cfg(windows)]
mod encoder;
mod input;
mod link;
#[cfg(windows)]
mod peer;
#[cfg(windows)]
mod sendinput;
#[cfg(windows)]
mod sessions;
#[cfg(windows)]
mod stream;
mod view;

#[cfg(windows)]
mod helper;

#[cfg(windows)]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    helper::run().await
}

#[cfg(not(windows))]
fn main() {
    eprintln!("codync-screen for Windows only builds its capture on Windows");
}
