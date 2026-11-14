//! `codync-host analytics on|off` against a real host: the running host hears the change, a
//! stopped one reads it when it starts, and a host running on another port is never written behind.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::json;
use std::path::Path;

/// `codync-host analytics <sharing> --port <port>` on `home`'s data: its stderr when it fails.
fn analytics(home: &Path, sharing: &str, port: &str) -> Result<(), String> {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_codync-host"))
        .args(["analytics", sharing, "--port", port])
        .env("CODYNC_HOME", home)
        .output()
        .expect("run codync-host analytics");
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).into_owned()) }
}

/// A port nothing answers on.
fn free_port() -> String {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port().to_string()
}

async fn sharing(host: &Host) -> bool {
    host.call("hello", json!({})).await["analytics"].as_bool().unwrap()
}

#[tokio::test]
async fn analytics_command_reaches_the_running_host_or_its_next_start() {
    let mut host = Host::start().await;
    let port = host.base.rsplit(':').next().unwrap().to_owned();
    assert!(sharing(&host).await, "on by default");

    analytics(&host.home, "off", &port).unwrap();
    assert!(!sharing(&host).await, "the running host heard it");

    let refused = analytics(&host.home, "on", &free_port()).unwrap_err();
    assert!(refused.contains("pass its --port"), "{refused}");
    assert!(!sharing(&host).await, "nothing saved behind the running host");

    host.restart(|home| analytics(home, "on", &free_port()).unwrap()).await;
    assert!(sharing(&host).await, "saved while stopped, read at start");
}
