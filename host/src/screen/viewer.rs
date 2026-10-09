//! Phones viewing the screen: prepared WebRTC sessions, offers, takeover and the on/off switch.

use super::{Control, KV_ENABLED, Screen, Viewer};
use crate::LockExt;
use crate::hub::Hub;
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

/// WebRTC ICE servers. Credentials are memory-only and must not be logged.
#[derive(Clone, Serialize, Deserialize)]
pub struct IceServer {
    pub urls: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IceConfig {
    pub ice_servers: Vec<IceServer>,
    pub expires_at: i64,
}

impl Default for IceConfig {
    fn default() -> Self {
        Self { ice_servers: vec![], expires_at: crate::store::now_ms() + 3_600_000 }
    }
}

impl Screen {
    /// Reserve one session before the phone gathers ICE; the host keeps the exact configuration.
    pub fn prepare(&self, owner: &str, ice: IceConfig, trickle: bool) -> Result<Value> {
        self.with_link(|link| {
            let trickle = trickle && self.status.locked().trickle;
            let mut sessions = self.sessions.locked();
            let now = crate::store::now_ms();
            sessions.retain(|_, v| v.started || v.pending_until > now);
            if sessions.len() >= 32 || sessions.values().filter(|v| v.owner == owner).count() >= 4 {
                bail!("too many screen sessions; close another screen and try again");
            }
            let session = uuid::Uuid::new_v4().to_string();
            let result = json!({"session": session, "iceServers": ice.ice_servers, "expiresAt": ice.expires_at, "trickle": trickle});
            sessions.insert(
                session,
                Viewer {
                    owner: owner.to_owned(),
                    ice,
                    started: false,
                    pending_until: now + 60_000,
                    link_id: link.id,
                    signaling: trickle.then(super::signaling::Signaling::new),
                },
            );
            Ok(result)
        })
    }

    pub fn owns_session(&self, session: &str, owner: &str) -> bool {
        self.sessions.locked().get(session).is_some_and(|v| v.owner == owner)
    }

    /// Answers a prepared session or a direct offer from an older client.
    pub async fn offer(&self, owner: &str, sdp: &str, session: Option<&str>, display: Option<u32>) -> Result<Value> {
        let link = self.link()?;
        if !self.status.locked().capture {
            bail!("Codync isn't allowed to record this computer's screen yet. Allow it in System Settings there.");
        }
        let session = match session {
            Some(id) => id.to_owned(),
            None => self.prepare(owner, IceConfig::default(), false)?["session"]
                .as_str()
                .context("missing screen session")?
                .to_owned(),
        };
        let (ice, trickle) = {
            let mut sessions = self.sessions.locked();
            let viewer = sessions.get_mut(&session).filter(|v| v.owner == owner).context("unknown screen session")?;
            let now = crate::store::now_ms();
            if viewer.ice.expires_at <= now || (!viewer.started && viewer.pending_until <= now) {
                bail!("screen connection expired; reconnect to continue");
            }
            if viewer.link_id != link.id {
                bail!("screen helper changed; reconnect");
            }
            let trickle = viewer.signaling.is_some();
            if let Some(signaling) = &mut viewer.signaling {
                signaling.begin_offer()?;
            }
            (viewer.ice.clone(), trickle)
        };
        let remote = !ice.ice_servers.is_empty();
        let res = link
            .request(
                "answer",
                json!({
                    "session": session, "sdp": sdp, "trickle": trickle, "display": display, "iceServers": ice.ice_servers,
                    "maxBitrateBps": if remote { 4_000_000 } else { 16_000_000 },
                    "maxFramerate": if remote { 30 } else { 60 },
                }),
            )
            .await;
        let res = match res {
            Ok(res) => res,
            Err(error) => {
                // A helper can finish after the RPC times out. Keep retrying close until it acknowledges.
                if let Some(viewer) = self.sessions.locked().get_mut(&session) {
                    viewer.ice.expires_at = 0;
                }
                if let Err(close_error) = self.close(&session).await {
                    tracing::warn!(error = format!("{close_error:#}"), "couldn't close failed screen session");
                }
                return Err(error);
            }
        };
        let answer = res["sdp"].as_str().ok_or_else(|| anyhow!("the screen helper sent no answer"))?;
        let accepted = {
            let mut sessions = self.sessions.locked();
            if let Some(viewer) = sessions.get_mut(&session) {
                if let Some(signaling) = &mut viewer.signaling {
                    signaling.answered()?;
                }
                viewer.started = true;
                true
            } else {
                false
            }
        };
        if !accepted || !self.enabled() {
            self.close(&session).await?;
            bail!("screen session closed while connecting");
        }
        self.emit();
        Ok(json!({"session": session, "sdp": answer}))
    }

    pub async fn close(&self, session: &str) -> Result<()> {
        self.fail_signaling(session, "screen session closed");
        let link = self.link.locked().clone();
        if let Some(link) = link {
            link.request("close", json!({"session": session})).await?;
        }
        self.session_closed(session);
        Ok(())
    }

    pub(super) fn session_closed(&self, session: &str) {
        if self.remove_session(session) {
            self.emit();
        }
    }

    pub(super) fn remove_session(&self, session: &str) -> bool {
        let mut sessions = self.sessions.locked();
        let removed = sessions.remove(session).is_some();
        if sessions.is_empty() {
            // Nobody is watching: hand control back to the bots.
            self.control.locked().user = false;
        }
        removed
    }

    pub fn takeover(&self, on: bool) {
        self.control.locked().user = on;
        self.emit();
    }

    /// Only callable from this computer (see `api/devices.rs`).
    pub async fn set_enabled(&self, store: &crate::store::Store, on: bool) -> Result<()> {
        if on && cfg!(windows) {
            anyhow::bail!("Remote screen isn't available on Windows yet.");
        }
        store.kv_set(KV_ENABLED, if on { "1" } else { "0" })?;
        *self.enabled.locked() = Some(on);
        if !on {
            let link = self.link.locked().clone();
            if let Some(link) = link
                && let Err(error) = link.request("closeAll", json!({})).await
            {
                tracing::warn!(error = format!("{error:#}"), "couldn't close screen sessions");
            }
            self.sessions.locked().clear();
            *self.control.locked() = Control::default();
        }
        self.emit();
        Ok(())
    }
}

/// Media bypasses the API, so keep checking the device lease while a viewer is connected.
pub async fn watch_viewer(hub: Arc<Hub>, session: String, owner: String) {
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let expired = {
            let sessions = hub.screen.sessions.locked();
            let Some(viewer) = sessions.get(&session) else { return };
            let now = crate::store::now_ms();
            viewer.ice.expires_at <= now || (!viewer.started && viewer.pending_until <= now)
        };
        let authorized = owner == "local"
            || crate::api::devices::authorize(&hub, &owner)
                .is_ok_and(|d| d.scopes.contains(&crate::store::Scope::Screen));
        if expired || !authorized || !hub.screen.enabled() {
            if let Err(error) = hub.screen.close(&session).await {
                tracing::warn!(error = format!("{error:#}"), "couldn't close expired screen session");
                continue;
            }
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::helper::Link;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicI64;
    use tokio::sync::{broadcast, mpsc};

    #[test]
    fn prepared_sessions_are_bounded_and_do_not_count_as_viewers() {
        let (tx, _) = broadcast::channel(8);
        let screen = Screen::new(Some(true), tx);
        let (tx, _) = mpsc::unbounded_channel();
        *screen.link.locked() =
            Some(Arc::new(Link { id: 1, tx, pending: Mutex::default(), next_id: AtomicI64::new(1) }));
        let config = IceConfig {
            ice_servers: vec![IceServer {
                urls: vec!["turns:turn.cloudflare.com:443?transport=tcp".into()],
                username: Some("u".into()),
                credential: Some("secret".into()),
            }],
            ..IceConfig::default()
        };
        let prepared = screen.prepare("phone", config, false).unwrap();
        let id = prepared["session"].as_str().unwrap();
        assert!(screen.owns_session(id, "phone"));
        assert!(!screen.owns_session(id, "other"));
        assert_eq!(prepared["iceServers"][0]["credential"], "secret");
        assert_eq!(screen.state()["viewers"], 0);
        assert!(!screen.state().to_string().contains("secret"));
        for _ in 0..3 {
            screen.prepare("phone", IceConfig::default(), false).unwrap();
        }
        assert!(screen.prepare("phone", IceConfig::default(), false).is_err());
        for viewer in screen.sessions.locked().values_mut() {
            viewer.pending_until = 0;
        }
        assert!(screen.prepare("phone", IceConfig::default(), false).is_ok());
        *screen.enabled.locked() = Some(false);
        assert!(screen.prepare("phone", IceConfig::default(), false).is_err());
    }
}
