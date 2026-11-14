//! One xdg `RemoteDesktop` session with screen cast sources: input injection and
//! PipeWire streams for every monitor. Persisted with a restore token so the user
//! approves once.

use crate::backend::{self, Display};
use anyhow::{Context, Result, anyhow};
use ashpd::desktop::PersistMode;
use ashpd::desktop::Session;
use ashpd::desktop::remote_desktop::{DeviceType, RemoteDesktop, SelectDevicesOptions};
use ashpd::desktop::screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType};
use futures_util::StreamExt;
use gst::prelude::*;
use serde_json::Value;
use std::os::fd::{IntoRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone)]
pub struct Portal(Arc<Inner>);

struct Inner {
    remote: RemoteDesktop,
    screencast: Screencast,
    session: Session<RemoteDesktop>,
    /// Each monitor with its PipeWire node.
    displays: Vec<(Display, u32)>,
    input: bool,
    active: AtomicBool,
}

impl Portal {
    pub async fn start(data: &Path, restore: bool) -> Result<Self> {
        let token_file = data.join("screen-restore-token");
        let token = if restore {
            std::fs::read_to_string(&token_file).ok()
        } else {
            None
        };
        let remote = RemoteDesktop::new()
            .await
            .context("the RemoteDesktop portal isn't available")?;
        let screencast = Screencast::new()
            .await
            .context("the ScreenCast portal isn't available")?;
        let session = remote.create_session(Default::default()).await?;
        remote
            .select_devices(
                &session,
                SelectDevicesOptions::default()
                    .set_devices(DeviceType::Keyboard | DeviceType::Pointer)
                    .set_persist_mode(PersistMode::ExplicitlyRevoked)
                    .set_restore_token(token.as_deref().map(str::trim)),
            )
            .await?;
        screencast
            .select_sources(
                &session,
                SelectSourcesOptions::default()
                    .set_cursor_mode(CursorMode::Embedded)
                    .set_sources(ashpd::enumflags2::BitFlags::from(SourceType::Monitor))
                    .set_multiple(true),
            )
            .await?;
        let started = remote
            .start(&session, None, Default::default())
            .await?
            .response()
            .context("remote access to this screen was not allowed")?;
        if let Some(t) = started.restore_token() {
            save_token(&token_file, t);
        }
        let displays = started
            .streams()
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let (x, y) = s.position().unwrap_or((0, 0));
                let (w, h) = s.size().unwrap_or((1920, 1080));
                let display = Display {
                    id: u32::try_from(i).unwrap_or(0),
                    x,
                    y,
                    width: f64::from(w),
                    height: f64::from(h),
                };
                (display, s.pipe_wire_node_id())
            })
            .collect::<Vec<_>>();
        if displays.is_empty() {
            return Err(anyhow!("the portal shared no screen"));
        }
        let input = started
            .devices()
            .contains(DeviceType::Pointer | DeviceType::Keyboard);
        let portal = Self(Arc::new(Inner {
            remote,
            screencast,
            session,
            displays,
            input,
            active: AtomicBool::new(true),
        }));
        // Ending sharing in the system UI must make setup available again.
        let watched = portal.clone();
        tokio::spawn(async move {
            match watched.session().receive_closed().await {
                Ok(mut closed) => {
                    let _ = closed.next().await;
                }
                Err(error) => tracing::warn!(%error, "could not watch the screen sharing session"),
            }
            watched.0.active.store(false, Ordering::Relaxed);
        });
        Ok(portal)
    }

    pub fn is_active(&self) -> bool {
        self.0.active.load(Ordering::Relaxed)
    }

    pub fn has_input(&self) -> bool {
        self.is_active() && self.0.input
    }

    pub fn status(&self) -> Value {
        let displays: Vec<Display> = self.0.displays.iter().map(|(d, _)| d.clone()).collect();
        backend::status(&displays, self.is_active(), self.has_input())
    }

    pub fn display(&self, id: Option<u64>) -> Result<Display> {
        let id = id.map_or(0, |i| u32::try_from(i).unwrap_or(u32::MAX));
        self.stream(id).map(|(d, _)| d.clone())
    }

    /// The PipeWire node that streams `d`.
    pub fn node(&self, d: &Display) -> Result<u32> {
        self.stream(d.id).map(|(_, node)| *node)
    }

    fn stream(&self, id: u32) -> Result<&(Display, u32)> {
        self.0
            .displays
            .iter()
            .find(|(d, _)| d.id == id)
            .ok_or_else(|| anyhow!("no such display"))
    }

    /// The display's stream on a fresh PipeWire remote (each pipeline owns its fd).
    pub async fn source(&self, d: &Display) -> Result<gst::Element> {
        let fd = self
            .0
            .screencast
            .open_pipe_wire_remote(&self.0.session, Default::default())
            .await?;
        capture_source(fd, self.node(d)?)
    }

    pub fn remote(&self) -> &RemoteDesktop {
        &self.0.remote
    }

    pub fn session(&self) -> &Session<RemoteDesktop> {
        &self.0.session
    }
}

/// The PipeWire source for one portal stream. Only properties every supported PipeWire has
/// (Ubuntu 24.04's 1.0 onward) go in the builder: an unknown one panics there.
pub fn capture_source(fd: OwnedFd, node: u32) -> Result<gst::Element> {
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

/// Whether this session offers the `RemoteDesktop` and `ScreenCast` portals (D-Bus may start
/// xdg-desktop-portal to answer). Without them the helper shares the X display directly.
pub async fn available() -> bool {
    match probe().await {
        Ok(()) => true,
        Err(error) => {
            tracing::debug!(
                error = format!("{error:#}"),
                "no screen sharing portal on this session"
            );
            false
        }
    }
}

async fn probe() -> Result<()> {
    let conn = zbus::Connection::session().await?;
    let properties = zbus::fdo::PropertiesProxy::builder(&conn)
        .destination("org.freedesktop.portal.Desktop")?
        .path("/org/freedesktop/portal/desktop")?
        .build()
        .await?;
    for interface in [
        "org.freedesktop.portal.RemoteDesktop",
        "org.freedesktop.portal.ScreenCast",
    ] {
        properties
            .get(
                zbus::names::InterfaceName::from_static_str(interface)?,
                "version",
            )
            .await
            .with_context(|| format!("{interface} isn't offered"))?;
    }
    Ok(())
}

fn save_token(path: &PathBuf, token: &str) {
    use std::os::unix::fs::PermissionsExt;
    if let Err(error) = std::fs::write(path, token) {
        tracing::warn!(%error, "couldn't save the portal restore token");
        return;
    }
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}
