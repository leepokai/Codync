//! The video thread of one viewer: capture, encode and hand H.264 access units to the async side.

use crate::capture::{Capture, Grab};
use crate::displays::Monitor;
use crate::encoder::Encoder;
use crate::view::View;
use anyhow::Result;
use bytes::Bytes;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

/// A still screen sends a repeat this often, so a viewer that joined late or lost a packet recovers.
const REPEAT: Duration = Duration::from_millis(1000);
/// How long to wait before trying to capture again after the desktop was unavailable (lock screen, UAC).
const RETRY: Duration = Duration::from_millis(500);

pub struct Frame {
    pub data: Bytes,
    pub duration: Duration,
}

/// What the async side tells the video thread.
#[derive(Default)]
pub struct Control {
    pub stop: AtomicBool,
    pub keyframe: AtomicBool,
    /// The newest `view` message (crop and viewport) not applied yet.
    pub view: Mutex<Option<Value>>,
}

impl Control {
    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }
}

pub fn spawn(
    monitor: Monitor,
    fps: u32,
    bitrate: u32,
    control: Arc<Control>,
    frames: mpsc::Sender<Frame>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        // SAFETY: first COM call on this thread.
        if let Err(error) = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok() } {
            tracing::error!(%error, "COM failed on the video thread");
            return;
        }
        if let Err(error) = run(&monitor, fps, bitrate, &control, &frames) {
            tracing::warn!(error = format!("{error:#}"), "video stopped");
        }
    })
}

fn run(
    monitor: &Monitor,
    fps: u32,
    bitrate: u32,
    control: &Control,
    frames: &mpsc::Sender<Frame>,
) -> Result<()> {
    let interval = Duration::from_secs(1) / fps;
    let mut view = View::whole(monitor.width, monitor.height);
    let mut capture = open(monitor, view, control);
    let mut encoder = capture
        .as_ref()
        .map(|_| Encoder::new(view.size.0, view.size.1, fps, bitrate))
        .transpose()?;
    let mut last_frame = Instant::now();
    let mut last_encode = Instant::now()
        .checked_sub(interval)
        .unwrap_or_else(Instant::now);
    control.keyframe.store(true, Ordering::Relaxed);
    while !control.stopped() {
        let Some(active) = capture.as_mut() else {
            // The desktop can't be captured right now: keep trying.
            std::thread::sleep(RETRY);
            capture = open(monitor, view, control);
            encoder = capture
                .as_ref()
                .map(|_| Encoder::new(view.size.0, view.size.1, fps, bitrate))
                .transpose()?;
            control.keyframe.store(true, Ordering::Relaxed);
            continue;
        };
        if let Some(message) = control
            .view
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            view = View::apply(&message, monitor.width, monitor.height);
            active.set_view(view)?;
            encoder = Some(Encoder::new(view.size.0, view.size.1, fps, bitrate)?);
            control.keyframe.store(true, Ordering::Relaxed);
        }
        if let Some(wait) = interval.checked_sub(last_encode.elapsed()) {
            std::thread::sleep(wait);
        }
        let key = control.keyframe.swap(false, Ordering::Relaxed);
        let fresh = match active.grab(interval) {
            Ok(Grab::Frame) => true,
            Ok(Grab::Idle) => false,
            Err(error) => {
                tracing::warn!(error = format!("{error:#}"), "capture failed, retrying");
                capture = None;
                continue;
            }
        };
        if !fresh && !key && last_frame.elapsed() < REPEAT {
            continue;
        }
        if !fresh {
            active.redraw()?;
        }
        let Some(encoder) = encoder.as_mut() else {
            continue;
        };
        if key {
            encoder.force_keyframe()?;
        }
        let duration = last_encode.elapsed().min(Duration::from_secs(1));
        last_encode = Instant::now();
        for unit in encoder.encode(&active.frame.data)? {
            last_frame = Instant::now();
            if frames
                .try_send(Frame {
                    data: Bytes::from(unit),
                    duration,
                })
                .is_err()
            {
                // The network can't keep up: dropping a P-frame needs a fresh keyframe.
                control.keyframe.store(true, Ordering::Relaxed);
            }
        }
    }
    Ok(())
}

fn open(monitor: &Monitor, view: View, control: &Control) -> Option<Capture> {
    match Capture::new(monitor, view) {
        Ok(capture) => Some(capture),
        Err(error) => {
            if !control.stopped() {
                tracing::info!(
                    error = format!("{error:#}"),
                    "can't capture the desktop yet"
                );
            }
            None
        }
    }
}
