//! Explicit, local-only permission requests from the computer access setup flow.

use super::Screen;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum Permission {
    Capture,
    Input,
    Portal,
}

impl Screen {
    pub(crate) async fn request_permission(&self, body: &Value) -> Result<Value> {
        let permission: Permission = serde_json::from_value(body["permission"].clone())
            .context("permission must be capture, input or portal")?;
        if cfg!(windows) {
            bail!("Windows does not require these computer access permissions");
        }
        let is_portal = matches!(permission, Permission::Portal);
        if is_portal != cfg!(target_os = "linux") {
            bail!("this permission is not supported on this platform");
        }
        self.link()?.request("requestPermission", json!({"permission": permission})).await?;
        Ok(self.state())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn invalid_permission_never_reaches_a_helper() {
        let screen = Screen::new(Some(true), tokio::sync::broadcast::channel(1).0);
        let error = screen.request_permission(&json!({"permission": "all"})).await.unwrap_err();
        assert!(error.to_string().contains("permission must be"));
    }

    #[test]
    fn remote_devices_cannot_request_system_permissions() {
        use crate::api::devices::{Caller, permit};
        assert!(permit(&Caller::Local, "requestScreenPermission").is_ok());
        let device = Caller::Pairing { key: "test".into() };
        assert!(permit(&device, "requestScreenPermission").is_err());
    }
}
