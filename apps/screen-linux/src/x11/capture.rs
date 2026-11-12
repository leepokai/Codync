//! One capture of the X display, shared by every stream and still. GStreamer's `ximagesrc` grabs
//! the X server for each frame and, inside the grab, takes Xlib's process-wide lock (XFixes for
//! the pointer, XShm for the image). A second `ximagesrc` opening its display at that moment holds
//! the same lock while it waits for the server's answer, which the grab holds back: neither moves
//! again, and the X server serves no other client (every app on that desktop freezes). So there is
//! one `ximagesrc` per display, fanned out to an `appsrc` per pipeline, and a new one opens only
//! once the previous one has stopped.

use anyhow::{Context, Result, anyhow};
use gst::glib::WeakRef;
use gst::prelude::*;
use std::sync::{Arc, Mutex, PoisonError};

type Outputs = Arc<Mutex<Vec<WeakRef<gst_app::AppSrc>>>>;

pub(super) struct Capture {
    pipeline: gst::Pipeline,
    /// The pipelines' sources; one dropped with its pipeline is pruned at the next frame.
    outputs: Outputs,
}

impl Capture {
    /// The whole screen of `display` with its pointer, at 30 fps. ponytail: each frame copies the
    /// full screen (no damage tracking); follow the viewers' `maxFramerate` if 60 is ever needed.
    pub(super) fn start(display: &str) -> Result<Self> {
        let missing =
            || anyhow!("GStreamer's X11 capture (ximagesrc, gstreamer1.0-plugins-good) is missing");
        let src = gst::ElementFactory::make("ximagesrc")
            .property("display-name", display)
            .property("use-damage", false)
            .property("show-pointer", true)
            .build()
            .map_err(|_| missing())?;
        let rate = gst::ElementFactory::make("capsfilter")
            .property(
                "caps",
                gst::Caps::builder("video/x-raw")
                    .field("framerate", gst::Fraction::new(30, 1))
                    .build(),
            )
            .build()?;
        let sink = gst_app::AppSink::builder()
            .sync(false)
            .max_buffers(1)
            .drop(true)
            .build();
        let pipeline = gst::Pipeline::new();
        pipeline.add_many([&src, &rate, sink.upcast_ref()])?;
        gst::Element::link_many([&src, &rate, sink.upcast_ref()])
            .context("can't capture this X display")?;
        let outputs = Outputs::default();
        let to = outputs.clone();
        sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    if let (Some(frame), Some(caps)) = (sample.buffer(), sample.caps()) {
                        fan_out(&to, frame, caps);
                    }
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );
        pipeline
            .set_state(gst::State::Playing)
            .context("can't start the X display's capture")?;
        Ok(Self { pipeline, outputs })
    }

    /// A live source of this capture's frames for one pipeline. It keeps no more than the newest
    /// two frames, so a slow encoder drops frames rather than memory piling up.
    pub(super) fn output(&self) -> gst::Element {
        let src = gst_app::AppSrc::builder()
            .is_live(true)
            .format(gst::Format::Time)
            .do_timestamp(true)
            .max_buffers(2)
            .leaky_type(gst_app::AppLeakyType::Downstream)
            .build();
        self.outputs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(src.downgrade());
        src.upcast()
    }

    /// Whether no pipeline takes its frames any more.
    pub(super) fn idle(&self) -> bool {
        let mut outputs = self.outputs.lock().unwrap_or_else(PoisonError::into_inner);
        outputs.retain(|o| o.upgrade().is_some());
        outputs.is_empty()
    }
}

impl Drop for Capture {
    /// Stopped before the next capture opens its display.
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

/// `frame` into every live output, stamped by each output's own pipeline clock.
fn fan_out(
    outputs: &Mutex<Vec<WeakRef<gst_app::AppSrc>>>,
    frame: &gst::BufferRef,
    caps: &gst::CapsRef,
) {
    let mut outputs = outputs.lock().unwrap_or_else(PoisonError::into_inner);
    outputs.retain(|o| o.upgrade().is_some());
    for out in outputs.iter().filter_map(WeakRef::upgrade) {
        if out.caps().is_none_or(|c| c.as_ref() != caps) {
            out.set_caps(Some(&caps.to_owned()));
        }
        // Shares the frame's memory; only the timestamps are its own.
        let mut buffer = frame.copy();
        let b = buffer.make_mut();
        b.set_pts(gst::ClockTime::NONE);
        b.set_dts(gst::ClockTime::NONE);
        let _ = out.push_buffer(buffer);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn frames_reach_each_output_until_none_is_left() {
        gst::init().unwrap();
        let capture = Capture {
            pipeline: gst::Pipeline::new(),
            outputs: Outputs::default(),
        };
        assert!(capture.idle());
        let src = capture.output();
        assert!(!capture.idle());
        let sink = gst_app::AppSink::builder().build();
        let pipeline = gst::Pipeline::new();
        pipeline.add_many([&src, sink.upcast_ref()]).unwrap();
        src.link(&sink).unwrap();
        pipeline.set_state(gst::State::Playing).unwrap();

        let caps = gst::Caps::builder("video/x-raw")
            .field("format", "BGRx")
            .field("width", 2)
            .field("height", 2)
            .field("framerate", gst::Fraction::new(30, 1))
            .build();
        let frame = gst::Buffer::from_slice(vec![0u8; 16]);
        fan_out(&capture.outputs, &frame, &caps);
        let sample = sink
            .try_pull_sample(gst::ClockTime::from_seconds(2))
            .expect("the output got the frame");
        assert_eq!(sample.caps(), Some(&*caps));
        assert_eq!(sample.buffer().map(gst::BufferRef::size), Some(16));

        pipeline.set_state(gst::State::Null).unwrap();
        drop((pipeline, src, sink));
        assert!(capture.idle(), "its pipeline gone, the capture can stop");
    }
}
