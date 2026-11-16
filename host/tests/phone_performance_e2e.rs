//! The iPhone's own store and channel against a real host: an app open's first milestones reach the
//! host's `phone-performance.jsonl`. The phone side is `PhonePerformanceDeliveryTests.swift`, run on a
//! simulator. Runs only when asked (macOS, Xcode):
//! `CODYNC_IOS_SIMULATOR=<udid> cargo test --test phone_performance_e2e -- --ignored`
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: failing loudly is the point

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "drives an iOS Simulator: CODYNC_IOS_SIMULATOR=<udid>"]
async fn an_iphone_open_reaches_the_hosts_performance_log() {
    let simulator = std::env::var("CODYNC_IOS_SIMULATOR").expect("CODYNC_IOS_SIMULATOR names the simulator to test on");
    let host = Host::start().await;
    host.bot("Performance bot", "auto").await;
    let pairing = host.call("pairing", json!({})).await;
    let dir = host.home.join("handoff");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("pairing"), loopback_pairing(pairing["pairingUrl"].as_str().unwrap(), &host.base)).unwrap();

    let mut phone = Phone::start(&simulator, &dir);
    approve_pairing(&host, &mut phone).await;
    let status = tokio::task::spawn_blocking(move || phone.0.wait()).await.unwrap().unwrap();
    assert!(status.success(), "the phone's test failed: see the xcodebuild output above");

    let log = host.home.join("phone-performance.jsonl");
    common::eventually("the phone's open timing reaches the host", Duration::from_secs(10), async || {
        phases(&log).iter().any(|p| p == "link_ready") && phases(&log).iter().any(|p| p == "hello")
    })
    .await;
    for line in lines(&log) {
        assert_eq!(line["type"], "open_timing");
        assert_eq!(line["platform"], "ios");
        assert_eq!(line["open"], "cold");
    }
    let phases = phases(&log);
    assert!(phases.iter().any(|p| p == "link_ready"), "{phases:?}");
    assert!(phases.iter().any(|p| p == "hello"), "{phases:?}");
}

/// The pairing QR with its addresses pointed at the host's loopback port: a simulator shares the Mac's
/// network, while the LAN addresses a host hands out don't answer it.
fn loopback_pairing(url: &str, base: &str) -> String {
    let encoded = base.replace(':', "%3A").replace('/', "%2F");
    let start = url.find("&urls=").expect("a pairing URL names its addresses") + "&urls=".len();
    let end = url[start..].find('&').map_or(url.len(), |i| start + i);
    format!("{}{}{}", &url[..start], encoded, &url[end..])
}

fn lines(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// Each logged line's `phase`, in order.
fn phases(path: &Path) -> Vec<String> {
    lines(path).iter().filter_map(|line| line["phase"].as_str().map(str::to_owned)).collect()
}

/// Lets the simulator in: a computer in no account asks before a phone pairs.
async fn approve_pairing(host: &Host, phone: &mut Phone) {
    common::eventually("the phone asks to pair", Duration::from_mins(10), async || {
        assert!(phone.0.try_wait().unwrap().is_none(), "the phone's test ended before pairing");
        let requests = host.call("pairingRequests", json!({})).await;
        let Some(request) = requests["requests"].as_array().and_then(|r| r.first()).cloned() else { return false };
        host.call("decidePairing", json!({"requestId": request["requestId"], "approve": true, "join": false})).await;
        true
    })
    .await;
}

/// The Swift side, running until it finishes; the build before it is part of the wait for its first step.
/// Stopped when dropped, so a failure on the host's side doesn't leave it running.
struct Phone(Child);

impl Phone {
    fn start(simulator: &str, dir: &Path) -> Self {
        let kit = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../apps/ios/Kit");
        let child = Command::new("xcodebuild")
            .current_dir(&kit)
            .args(["test", "-scheme", "CodyncKit-Package", "-derivedDataPath", "../../../build/dd"])
            .args(["-destination", &format!("platform=iOS Simulator,id={simulator}")])
            .arg("-only-testing:CodyncUITests/aPhoneOpenSendsItsTimingToTheHost()")
            .env("TEST_RUNNER_CODYNC_PERFORMANCE_DIR", dir)
            .env("TEST_RUNNER_CODYNC_RECORD_PERFORMANCE", "1")
            .stdout(Stdio::inherit())
            .spawn()
            .expect("run xcodebuild");
        Self(child)
    }
}

impl Drop for Phone {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            common::stop(&mut self.0);
        }
    }
}
