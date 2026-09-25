//! One xdg `RemoteDesktop` session with screen cast sources: input injection and
//! PipeWire streams for every monitor. Persisted with a restore token so the user
//! approves once.

use anyhow::{Context, Result, anyhow};
use ashpd::desktop::PersistMode;
use ashpd::desktop::Session;
use ashpd::desktop::remote_desktop::{DeviceType, RemoteDesktop, SelectDevicesOptions};
use ashpd::desktop::screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType};
use serde_json::{Value, json};
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A monitor as the portal streams it. `width`/`height` are logical pixels: the
/// coordinate space of input events and of the host's `Display` points.
#[derive(Clone, Debug)]
pub struct Display {
    pub id: u32,
    pub node: u32,
    pub x: i32,
    pub y: i32,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone)]
pub struct Portal(Arc<Inner>);

struct Inner {
    remote: RemoteDesktop,
    screencast: Screencast,
    session: Session<RemoteDesktop>,
    displays: Vec<Display>,
    input: bool,
}

impl Portal {
    pub async fn start(data: &Path) -> Result<Self> {
        let token_file = data.join("screen-restore-token");
        let token = std::fs::read_to_string(&token_file).ok();
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
                Display {
                    id: u32::try_from(i).unwrap_or(0),
                    node: s.pipe_wire_node_id(),
                    x,
                    y,
                    width: f64::from(w),
                    height: f64::from(h),
                }
            })
            .collect::<Vec<_>>();
        if displays.is_empty() {
            return Err(anyhow!("the portal shared no screen"));
        }
        let input = started.devices().contains(DeviceType::Pointer);
        Ok(Self(Arc::new(Inner {
            remote,
            screencast,
            session,
            displays,
            input,
        })))
    }

    pub fn status(&self) -> Value {
        json!({
            "platform": "linux",
            "version": env!("CARGO_PKG_VERSION"),
            "capture": true,
            "input": self.0.input,
            "displays": self.0.displays.iter().map(|d| json!({
                "id": d.id,
                "name": format!("Display {}", d.id + 1),
                "width": d.width,
                "height": d.height,
                "main": d.id == 0,
            })).collect::<Vec<_>>(),
        })
    }

    pub fn display(&self, id: Option<u64>) -> Result<Display> {
        let id = id.map_or(0, |i| u32::try_from(i).unwrap_or(u32::MAX));
        self.0
            .displays
            .iter()
            .find(|d| d.id == id)
            .cloned()
            .ok_or_else(|| anyhow!("no such display"))
    }

    /// A fresh PipeWire remote for one consumer (each pipeline owns its fd).
    pub async fn pipewire_fd(&self) -> Result<OwnedFd> {
        Ok(self
            .0
            .screencast
            .open_pipe_wire_remote(&self.0.session, Default::default())
            .await?)
    }

    pub fn remote(&self) -> &RemoteDesktop {
        &self.0.remote
    }

    pub fn session(&self) -> &Session<RemoteDesktop> {
        &self.0.session
    }
}

fn save_token(path: &PathBuf, token: &str) {
    use std::os::unix::fs::PermissionsExt;
    if let Err(error) = std::fs::write(path, token) {
        tracing::warn!(%error, "couldn't save the portal restore token");
        return;
    }
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}
