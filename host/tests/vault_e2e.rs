//! When a host makes its credential key, and where a disposable host keeps it (`market::vault`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::json;
use std::time::Duration;

/// The rule behind every other test staying out of the credential store: nothing opens it before a secret exists.
#[tokio::test]
async fn a_new_computer_makes_no_key_until_a_secret_is_saved() {
    let host = Host::start().await;
    common::eventually("the host made its lead bot", Duration::from_secs(30), async || {
        host.snapshot().await["bots"].as_array().unwrap().iter().any(|b| b["lead"] == true)
    })
    .await;
    let bot = host.call("createBot", json!({"name": "Plain", "backend": "custom", "command": "unused"})).await;
    assert_eq!(bot["bot"]["connectors"], json!([]));
    assert!(!host.home.join("vault.key").exists(), "a host with nothing saved opened its vault");

    host.call("credentialSaveLogin", json!({"site": "example.com", "username": "me", "password": "pw"})).await;
    assert!(host.home.join("vault.key").exists());
}

#[tokio::test]
async fn a_folder_key_outlives_a_restart_and_only_its_user_can_read_it() {
    let mut host = Host::start().await;
    host.call("credentialSaveLogin", json!({"site": "example.com", "username": "me", "password": "pw"})).await;
    let key = host.home.join("vault.key");
    assert_eq!(std::fs::read(&key).unwrap().len(), 32);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(std::fs::metadata(&key).unwrap().permissions().mode() & 0o777, 0o600);
    }
    host.restart(|_| {}).await;
    let logins = host.call("credentialLogins", json!({})).await;
    assert_eq!(logins["items"][0]["username"], "me", "{logins}");
}
