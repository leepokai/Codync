//! Where a stream's frames come from: a portal's PipeWire stream or an X display's root window.

use crate::portal::Display;
use crate::stream::make;
use anyhow::{Context, Result};
use gst::prelude::*;
use std::os::fd::{IntoRawFd, OwnedFd};

/// Where one pipeline's frames come from.
pub enum Capture {
    /// The portal's PipeWire remote for this pipeline.
    PipeWire(OwnedFd),
    /// The root window of this X display.
    X11(String),
}

/// `ximagesrc` for the whole screen at `fps` (it runs at 25 fps unless told). Damage tracking
/// is off so every frame is a full XShm copy: partial updates can leave stale regions.
fn x11_source(name: &str, fps: i32) -> Result<gst::Element> {
    let src = gst::ElementFactory::make("ximagesrc")
        .property("display-name", name)
        .property("use-damage", false)
        .property("show-pointer", true)
        .build()
        .context("GStreamer's ximagesrc (gstreamer1.0-plugins-good) is missing")?;
    let rate = make("capsfilter")?;
    rate.set_property(
        "caps",
        gst::Caps::builder("video/x-raw")
            .field("framerate", gst::Fraction::new(fps, 1))
            .build(),
    );
    let bin = gst::Bin::new();
    bin.add_many([&src, &rate])?;
    src.link(&rate)?;
    let pad = rate
        .static_pad("src")
        .context("capsfilter has no src pad")?;
    bin.add_pad(&gst::GhostPad::with_target(&pad)?)?;
    Ok(bin.upcast())
}

/// The PipeWire source for one portal stream. Only properties every supported PipeWire has
/// (Ubuntu 24.04's 1.0 onward) go in the builder: an unknown one panics there.
pub(crate) fn capture_source(fd: OwnedFd, node: u32) -> Result<gst::Element> {
    let src = gst::ElementFactory::make("pipewiresrc")
        .property("fd", fd.into_raw_fd())
        .property("path", node.to_string())
        .property("do-timestamp", true)
        .build()
        .context("GStreamer's PipeWire plugin (gstreamer1.0-pipewire) is missing")?;
    if src.has_property("keepalive-time") {
        // A still screen keeps sending frames now and then, so keyframe requests get answered.
        src.set_property("keepalive-time", 1000i32);
    }
    Ok(src)
}

impl Capture {
    /// The source element for one stream of `display` at `fps`.
    pub fn source(self, display: &Display, fps: i32) -> Result<gst::Element> {
        match self {
            Self::PipeWire(fd) => capture_source(fd, display.node),
            Self::X11(name) => x11_source(&name, fps),
        }
    }
}
