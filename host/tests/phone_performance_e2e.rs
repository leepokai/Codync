//! The phones' apps against a real host: an app open's first milestones and a scroll reach the host's
//! `phone-performance.jsonl`. The phone sides are `PhonePerformanceDeliveryUITests.swift`, run on a simulator,
//! and `PhonePerformanceDeliveryTest.kt`, run on an emulator. Run only when asked:
//! `CODYNC_IOS_SIMULATOR=<udid> cargo test --test phone_performance_e2e iphone -- --ignored` (macOS, Xcode);
//! `CODYNC_ANDROID_EMULATOR=<serial> cargo test --test phone_performance_e2e android -- --ignored` (JDK 21, Android SDK).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: failing loudly is the point

mod common;

use base64::Engine;
use common::host::Host;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "drives an iOS Simulator: CODYNC_IOS_SIMULATOR=<udid>"]
async fn an_iphone_open_and_scroll_reach_the_hosts_performance_log() {
    let simulator = std::env::var("CODYNC_IOS_SIMULATOR").expect("CODYNC_IOS_SIMULATOR names the simulator to test on");
    let host = performance_host().await;
    let pairing = loopback_pairing(&host).await;
    let dir = host.home.join("handoff");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("pairing"), pairing).unwrap();
    // A fresh install: the roster must hold only this host's bots.
    let _ = Command::new("xcrun").args(["simctl", "uninstall", &simulator, "com.pokai.Codync.ios.dev"]).status();

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut phone = Command::new("xcodebuild");
    phone
        .current_dir(&root)
        .args(["test", "-project", "apps/Codync.xcodeproj", "-scheme", "iOS Dev", "-derivedDataPath", "build/dd"])
        .args(["-destination", &format!("platform=iOS Simulator,id={simulator}")])
        .arg("-only-testing:iOSUITests/PhonePerformanceDeliveryUITests")
        .env("TEST_RUNNER_CODYNC_PERFORMANCE_DIR", &dir);
    run_phone(&host, phone).await;
    expect_open_and_scroll(&host, "ios").await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "drives an Android emulator: CODYNC_ANDROID_EMULATOR=<serial>"]
async fn an_android_open_and_scroll_reach_the_hosts_performance_log() {
    let emulator =
        std::env::var("CODYNC_ANDROID_EMULATOR").expect("CODYNC_ANDROID_EMULATOR names the emulator to test on");
    let host = performance_host().await;
    let pairing = loopback_pairing(&host).await;
    // The emulator's loopback is its own: map the host's port into it.
    let port = host.base.rsplit(':').next().unwrap();
    let adb = |args: &[&str]| {
        let status = Command::new("adb").args(["-s", &emulator]).args(args).status().expect("run adb");
        assert!(status.success(), "adb {args:?} failed");
    };
    adb(&["reverse", &format!("tcp:{port}"), &format!("tcp:{port}")]);

    let android = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../apps/android");
    // `am instrument` runs through the device's shell, which would split the link at its `&`s.
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&pairing);
    let mut phone = Command::new("./gradlew");
    phone
        .current_dir(&android)
        .args([":app:connectedDebugAndroidTest", "--console=plain"])
        .arg("-Pandroid.testInstrumentationRunnerArguments.class=com.codync.android.PhonePerformanceDeliveryTest")
        .arg(format!("-Pandroid.testInstrumentationRunnerArguments.pairing={encoded}"))
        .env("ANDROID_SERIAL", &emulator);
    run_phone(&host, phone).await;
    adb(&["reverse", "--remove", &format!("tcp:{port}")]);
    expect_open_and_scroll(&host, "android").await;
}

async fn performance_host() -> Host {
    let host = Host::start().await;
    // Enough bots for a phone's roster to scroll; the lead bot makes it 30.
    for n in 1..=29 {
        host.bot(&format!("Performance bot {n}"), "auto").await;
    }
    host
}

/// The pairing QR with its addresses pointed at the host's loopback port: a simulator shares the Mac's
/// network and an emulator reaches it through `adb reverse`, while the LAN addresses a host hands out answer neither.
async fn loopback_pairing(host: &Host) -> String {
    let pairing = host.call("pairing", json!({})).await;
    let url = pairing["pairingUrl"].as_str().unwrap();
    let encoded = host.base.replace(':', "%3A").replace('/', "%2F");
    let start = url.find("&urls=").expect("a pairing URL names its addresses") + "&urls=".len();
    let end = url[start..].find('&').map_or(url.len(), |i| start + i);
    format!("{}{}{}", &url[..start], encoded, &url[end..])
}

/// Runs the phone's side to its end, letting it in when it asks to pair; the build before it is part of
/// the wait for its first step. Stopped when dropped, so a failure on the host's side doesn't leave it running.
async fn run_phone(host: &Host, mut command: Command) {
    let mut phone = Phone(command.stdout(Stdio::inherit()).spawn().expect("start the phone's test"));
    approve_pairing(host, &mut phone).await;
    let status = tokio::task::spawn_blocking(move || phone.0.wait()).await.unwrap().unwrap();
    assert!(status.success(), "the phone's test failed: see its output above");
}

/// A computer in no account asks before a phone pairs.
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

/// The log holds a cold open's first milestones and one scroll of the bot list, all from `platform`.
async fn expect_open_and_scroll(host: &Host, platform: &str) {
    let log = host.home.join("phone-performance.jsonl");
    common::eventually("the phone's timing reaches the host", Duration::from_secs(10), async || {
        let lines = lines(&log);
        ["link_ready", "hello"].iter().all(|phase| lines.iter().any(|line| line["phase"] == *phase))
            && lines.iter().any(|line| line["type"] == "scroll")
    })
    .await;
    for line in lines(&log) {
        assert_eq!(line["platform"], platform, "{line}");
        match line["type"].as_str() {
            Some("open_timing") => assert_eq!(line["open"], "cold", "{line}"),
            Some("scroll") => {
                assert_eq!(line["list"], "bots", "{line}");
                assert_eq!(line["rows"], 30, "{line}");
                assert!(line["frames"].as_u64().unwrap() > 0, "{line}");
            }
            _ => panic!("unexpected record: {line}"),
        }
    }
}

fn lines(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

struct Phone(Child);

impl Drop for Phone {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            common::stop(&mut self.0);
        }
    }
}
