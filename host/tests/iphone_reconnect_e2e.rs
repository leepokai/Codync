//! The iPhone store reconnecting through a relay to a host that restarts, with ~30 chats and days of
//! history: its catch-up is only what is new, and no relayed device link overflows. The phone side is
//! `HostRestartCatchUpTests.swift`, run on a simulator; the two hand each other each step through
//! files in one folder. The relay is `private-relay/`, run from this checkout (after its `npm ci`).
//! Runs only when asked (macOS, Xcode, Node):
//! `CODYNC_IOS_SIMULATOR=<udid> cargo test --test iphone_reconnect_e2e -- --ignored`
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: failing loudly is the point

mod common;

use common::host::Host;
use serde_json::json;
use std::ffi::OsStr;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

const BOTS: i64 = 30;
const MESSAGES_PER_BOT: i64 = 60;
const DAY_MS: i64 = 86_400_000;
const OVERFLOW: &str = "device link overflowed";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "drives an iOS Simulator: CODYNC_IOS_SIMULATOR=<udid>"]
async fn a_restarted_host_catches_the_iphone_up_only_on_what_is_new() {
    let simulator = std::env::var("CODYNC_IOS_SIMULATOR").expect("CODYNC_IOS_SIMULATOR names the simulator to test on");
    let relay = Relay::start();
    // The host listens on loopback only, so the LAN and Tailscale addresses it hands out refuse the phone: the relay it is.
    let mut host = Host::start_with(&[
        ("CODYNC_CLOUD", OsStr::new("on")),
        ("CODYNC_CLOUD_URL", OsStr::new(&relay.url)),
        ("CODYNC_CLOUD_TOKEN", OsStr::new(&relay.token)),
    ])
    .await;
    let bots = seed(&mut host).await;
    on_relay(&host).await;
    let dir = host.home.join("handoff");
    std::fs::create_dir_all(&dir).unwrap();
    let pairing = host.call("pairing", json!({})).await;
    std::fs::write(dir.join("pairing"), pairing["pairingUrl"].as_str().unwrap()).unwrap();
    let mut phone = Phone::start(&simulator, &dir);

    approve_pairing(&host, &mut phone).await;
    handoff(&mut phone, &dir, "synced").await;
    restart(&mut host, |_| {}).await;
    std::fs::write(dir.join("restarted"), "").unwrap();

    // Two messages arrive while the phone is closed, written as the host stops and starts again.
    handoff(&mut phone, &dir, "closed").await;
    let mut sent = vec![];
    restart(&mut host, |home| {
        let db = rusqlite::Connection::open(home.join("codync.db")).unwrap();
        sent = bots[..2].iter().map(|bot| insert(&db, bot, "while away", now_ms())).collect();
    })
    .await;
    std::fs::write(dir.join("sent"), sent.join(",")).unwrap();

    let status = tokio::task::spawn_blocking(move || phone.0.wait()).await.unwrap().unwrap();
    no_overflow(&host.home);
    assert!(status.success(), "the phone's test failed: see the xcodebuild output above");
}

/// `private-relay/` on a free loopback port, with its own data and registration token.
struct Relay {
    child: Child,
    data: PathBuf,
    url: String,
    token: String,
}

impl Relay {
    fn start() -> Self {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../private-relay");
        assert!(dir.join("node_modules").exists(), "run `npm ci` in private-relay first");
        let data = std::env::temp_dir().join(format!("codync-relay-e2e-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&data).unwrap();
        let token = uuid::Uuid::new_v4().simple().to_string();
        let mut child = Command::new("node")
            .current_dir(&dir)
            .args(["--experimental-strip-types", "src/index.ts"])
            .env("CODYNC_PRIVATE_RELAY_DATA", &data)
            .env("CODYNC_PRIVATE_RELAY_TOKEN", &token)
            .env("PORT", "0")
            .stdout(Stdio::piped())
            .spawn()
            .expect("run node");
        // Its one line on stdout, once it listens: `... listening on http://127.0.0.1:<port> for ...`. Ended before
        // it, the line is empty and the reason is on stderr above.
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
        let url =
            line.split_whitespace().find(|w| w.starts_with("http://")).expect("the relay didn't start").to_owned();
        Self { child, data, url, token }
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        common::stop(&mut self.child);
        let _ = std::fs::remove_dir_all(&self.data);
    }
}

/// Restarts the host, checking the run that ends overflowed no link, and waits until it is back on the relay.
async fn restart(host: &mut Host, between: impl FnOnce(&Path)) {
    host.restart(|home| {
        no_overflow(home);
        between(home);
    })
    .await;
    on_relay(host).await;
}

async fn on_relay(host: &Host) {
    common::eventually("the host is on the relay", Duration::from_secs(30), async || {
        host.call("cloudStatus", json!({})).await["connected"] == true
    })
    .await;
}

/// `BOTS` chats of `MESSAGES_PER_BOT` messages each over the past five days, written straight into
/// the database: sent through the host, each would start its bot's agent.
async fn seed(host: &mut Host) -> Vec<String> {
    let mut bots = vec![];
    for b in 0..BOTS {
        bots.push(host.bot(&format!("Bot {b}"), "auto").await);
    }
    host.restart(|home| {
        let db = rusqlite::Connection::open(home.join("codync.db")).unwrap();
        let total = BOTS * MESSAGES_PER_BOT;
        for m in 0..MESSAGES_PER_BOT {
            for (b, bot) in (0..).zip(&bots) {
                let age = total - (m * BOTS + b);
                insert(&db, bot, &format!("message {m}"), now_ms() - age * 5 * DAY_MS / total);
            }
        }
    })
    .await;
    bots
}

/// A user message in `bot`'s chat at `at`, stamped with the host's next rev as the host writes one: its id.
fn insert(db: &rusqlite::Connection, bot: &str, text: &str, at: i64) -> String {
    let rev: i64 = db
        .query_row(
            "INSERT INTO kv(k, v) VALUES('rev', '1') ON CONFLICT(k) DO UPDATE SET v = CAST(v AS INTEGER) + 1
             RETURNING CAST(v AS INTEGER)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    db.execute(
        "INSERT INTO entries(id, bot_id, rev, kind, turn, data, created_at, updated_at) VALUES(?1, ?2, ?3, 'user', 0, ?4, ?5, ?5)",
        rusqlite::params![id, bot, rev, json!({"text": text}).to_string(), at],
    )
    .unwrap();
    id
}

fn now_ms() -> i64 {
    i64::try_from(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()).unwrap()
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
            .arg("-only-testing:CodyncUITests/aRestartedHostCatchesTheIPhoneUpOnlyOnWhatIsNew()")
            .env("TEST_RUNNER_CODYNC_RESTART_DIR", dir)
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

/// Waits for the phone's `name` file; the phone's test ending first fails at once.
async fn handoff(phone: &mut Phone, dir: &Path, name: &str) {
    let file = dir.join(name);
    common::eventually(&format!("the phone writes {name}"), Duration::from_mins(10), async || {
        assert!(phone.0.try_wait().unwrap().is_none(), "the phone's test ended before writing {name}");
        file.exists()
    })
    .await;
}

fn no_overflow(home: &Path) {
    let log = std::fs::read_to_string(home.join("host.log")).unwrap_or_default();
    assert!(!log.contains(OVERFLOW), "a device link overflowed:\n{log}");
}
