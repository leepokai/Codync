//! Error tracking: panics, failures the host got past that should never happen, and uncaught client errors as
//! `PostHog` `$exception` events, sent with usage analytics (same switch, same identity). Never content: an
//! error's message is dropped (a panic keeps only a literal written in the code, or a known runtime panic's fixed
//! start), and a file path keeps only what lies inside the code base. What is collected:
//! docs/features/analytics.md.

use super::{Event, Msg, capture};
use crate::LockExt as _;
use crate::hub::Hub;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::panic::Location;
use std::sync::{Arc, Mutex};

/// The mechanism of a failure the host got past ([`unexpected`]).
const HANDLED: &str = "handled";

/// Frames kept per exception, the ones nearest the error.
const MAX_FRAMES: usize = 50;

/// One exception: what `reportError` takes, and what goes out as `$exception`.
#[derive(Debug, Serialize, Deserialize)]
pub struct Report {
    pub process: Process,
    /// The error's type: `Panic`, `TypeError`, `RendererGone`, …
    #[serde(rename = "type")]
    pub kind: String,
    /// How it was caught: `panic`, `onerror`, `unhandledrejection`, `render-process-gone`, …
    pub mechanism: String,
    /// A panic's literal message, or a reason the platform names (`oom`); never an error's text.
    #[serde(default)]
    pub value: String,
    /// Oldest call first, the failing one last.
    #[serde(default)]
    pub frames: Vec<Frame>,
}

/// Where the exception happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Process {
    Host,
    Tui,
    DesktopMain,
    DesktopRenderer,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Frame {
    pub function: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
}

impl Report {
    /// A panic at its location, with [`panic_message`]: `unwrap` on an error or a formatted
    /// `expect` can carry runtime data.
    // ponytail: no backtrace, release builds are stripped; upload debug symbols with
    // `posthog-cli` and send instruction addresses if the panic site isn't enough.
    pub fn panic(info: &std::panic::PanicHookInfo<'_>, process: Process) -> Self {
        let value = panic_message(info.payload());
        let frames = info
            .location()
            .map(|l| Frame {
                function: "panic".into(),
                file: Some(l.file().into()),
                line: Some(l.line()),
                column: Some(l.column()),
            })
            .into_iter()
            .collect();
        Self { process, kind: "Panic".into(), mechanism: "panic".into(), value, frames }
    }

    /// A failure the host got past, where it was noticed. `what` names it in both the type's place and the
    /// frame's, so each failure is an issue of its own whatever line a release moves it to.
    fn handled(what: &'static str, at: &Location<'_>) -> Self {
        let frame = Frame {
            function: what.into(),
            file: Some(at.file().into()),
            line: Some(at.line()),
            column: Some(at.column()),
        };
        Self {
            process: Process::Host,
            kind: "HostError".into(),
            mechanism: HANDLED.into(),
            value: what.into(),
            frames: vec![frame],
        }
    }

    /// The `$exception` properties (`PostHog`'s generic `custom` frames), with paths cut down.
    fn properties(self) -> Map<String, Value> {
        let lang = match self.process {
            Process::Host | Process::Tui => "rust",
            Process::DesktopMain | Process::DesktopRenderer => "javascript",
        };
        let skip = self.frames.len().saturating_sub(MAX_FRAMES);
        let frames: Vec<Value> = self
            .frames
            .into_iter()
            .skip(skip)
            .map(|f| {
                json!({
                    "platform": "custom",
                    "lang": lang,
                    "function": clip(&f.function, 200),
                    "filename": f.file.as_deref().map(code_path),
                    "lineno": f.line,
                    "colno": f.column,
                    "resolved": true,
                    "in_app": f.file.as_deref().is_some_and(in_app),
                })
            })
            .collect();
        let exception = json!({
            "type": token(&self.kind, "Error"),
            "value": clip(&self.value, 300),
            "mechanism": {"type": token(&self.mechanism, "generic"), "handled": self.mechanism == HANDLED, "synthetic": false},
            "stacktrace": {"type": "raw", "frames": frames},
        });
        let level = if self.kind == "Panic" { "fatal" } else { "error" };
        let process = serde_json::to_value(self.process).unwrap_or_default();
        let p = json!({"$exception_list": [exception], "$exception_level": level, "process": process});
        p.as_object().cloned().unwrap_or_default()
    }
}

/// `reportError`: an exception this computer's own app or terminal UI caught.
pub fn report(hub: &Hub, body: &Value) -> anyhow::Result<()> {
    let report = Report::deserialize(body)?;
    anyhow::ensure!(report.process != Process::Host, "unknown process");
    capture(hub, Event::Exception, report.properties());
    Ok(())
}

/// Reports the host's own panics: a panicking task or thread doesn't stop the host, so this
/// is often the only sign of one.
// ponytail: a panic that ends the process (the main thread) is lost with the queue; persist
// it and send it on the next start if that ever shows up.
pub fn report_panics(hub: &Arc<Hub>) {
    let hub = Arc::downgrade(hub);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(hub) = hub.upgrade() {
            // Captured by the analytics task: here it could wait on a lock the panic site holds.
            let _ = hub.analytics.tx.send(Msg::Report(Report::panic(info, Process::Host)));
        }
        previous(info);
    }));
}

/// Reports a failure the host got past but that should never happen (a box that won't save, a task that is gone):
/// `what` failed, a literal written in the code, and where it was noticed; never the error's text. Queued like a
/// panic, so it is safe under any lock.
// ponytail: once per place while the host runs, so a full disk isn't a flood; count them and send the count if how
// often one computer hits a place ever matters.
#[track_caller]
pub fn unexpected(hub: &Hub, what: &'static str) {
    let at = Location::caller();
    if first_time(at) {
        let _ = hub.analytics.tx.send(Msg::Report(Report::handled(what, at)));
    }
}

/// Logs a failure the host got past and reports it ([`unexpected`]) under one name, so the log's line and the issue
/// never drift apart: `host_error!(hub, "couldn't save an item's end", task = %cfg.id)`, the fields as
/// `tracing::error!` takes them. The place reported is the caller's.
macro_rules! host_error {
    ($hub:expr, $what:literal $(, $($fields:tt)+)?) => {{
        tracing::error!($($($fields)+,)? $what);
        $crate::analytics::errors::unexpected($hub, $what);
    }};
}
pub(crate) use host_error;

/// Whether nothing was reported from `at` since the host started.
fn first_time(at: &'static Location<'static>) -> bool {
    static SEEN: Mutex<BTreeSet<(&'static str, u32)>> = Mutex::new(BTreeSet::new());
    SEEN.locked().insert((at.file(), at.line()))
}

/// Captures what `report_panics` and [`unexpected`] queued.
pub(super) fn capture_report(hub: &Hub, report: Report) {
    capture(hub, Event::Exception, report.properties());
}

/// Panics the runtime formats with an OS error or a value: their fixed start says what failed.
/// `tokio::task::spawn_blocking` panics with the first one at its caller when no thread can start.
const RUNTIME_PANICS: [&str; 4] = [
    "OS can't spawn worker thread",
    "failed to spawn thread",
    "called `Result::unwrap()` on an `Err` value",
    "index out of bounds",
];

/// A literal panic message whole; a formatted one only as the fixed start of a [`RUNTIME_PANICS`] entry.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(literal) = payload.downcast_ref::<&'static str>() {
        return (*literal).to_owned();
    }
    let formatted = payload.downcast_ref::<String>().map_or("", String::as_str);
    RUNTIME_PANICS
        .iter()
        .find(|known| formatted.starts_with(**known))
        .map_or_else(String::new, |known| (*known).to_owned())
}

/// A frame's file without anything outside the code base: an absolute path (a home folder,
/// an install location) keeps only its file name.
fn code_path(file: &str) -> &str {
    let path = file.split_once("://").map_or(file, |(_, rest)| rest);
    let absolute = path.starts_with(['/', '\\']) || path.as_bytes().get(1) == Some(&b':');
    if absolute { path.rsplit(['/', '\\']).next().unwrap_or(path) } else { path }
}

/// Codync's own code, not a dependency's or the runtime's.
fn in_app(file: &str) -> bool {
    let path = file.split_once("://").map_or(file, |(_, rest)| rest);
    !(path.contains("node_modules") || file.starts_with("node:") || path.contains(".cargo") || path.contains("/rustc/"))
}

/// A type or mechanism name: identifier characters only.
fn token(s: &str, fallback: &str) -> String {
    let t: String = s.chars().filter(|c| c.is_ascii_alphanumeric() || "_.:-$".contains(*c)).take(80).collect();
    if t.is_empty() { fallback.into() } else { t }
}

fn clip(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_keep_only_the_code_base() {
        assert_eq!(code_path("src/agent/acp.rs"), "src/agent/acp.rs");
        assert_eq!(code_path("/Users/kevin/.cargo/registry/src/x/tokio-1.0/src/lib.rs"), "lib.rs");
        assert_eq!(
            code_path("file:///Users/kevin/Codync.app/Contents/Resources/app.asar/out/renderer/assets/index-a1.js"),
            "index-a1.js"
        );
        assert_eq!(code_path(r"C:\Users\kevin\AppData\Local\Codync\resources\app.asar\out\main\index.js"), "index.js");
        assert_eq!(code_path("file:///C:/Users/kevin/app.asar/out/renderer/index.js"), "index.js");
        assert!(in_app("src/agent/acp.rs"));
        assert!(!in_app("/Users/kevin/.cargo/registry/src/x/tokio-1.0/src/lib.rs"));
        assert!(!in_app("node:internal/process/task_queues"));
    }

    #[test]
    fn a_report_carries_no_content() {
        let body = json!({
            "process": "desktop-renderer",
            "type": "TypeError <script>",
            "mechanism": "onerror",
            "frames": [
                {"function": "render", "file": "file:///home/kevin/.mount_Codync/resources/app.asar/out/renderer/assets/index-a1.js", "line": 10, "column": 4},
                {"function": "loadChat", "file": "file:///home/kevin/.mount_Codync/resources/app.asar/out/renderer/assets/index-a1.js", "line": 99, "column": 2},
            ],
        });
        let p = Report::deserialize(&body).expect("a renderer report").properties();
        let text = Value::Object(p.clone()).to_string();
        assert!(!text.contains("kevin"), "{text}");
        let e = &p["$exception_list"][0];
        assert_eq!(e["type"], "TypeErrorscript");
        assert_eq!(e["value"], "");
        assert_eq!(e["stacktrace"]["frames"][1]["function"], "loadChat", "the failing frame stays last");
        assert_eq!(e["stacktrace"]["frames"][1]["lang"], "javascript");
        assert_eq!(p["process"], "desktop-renderer");
        assert_eq!(p["$exception_level"], "error");
    }

    #[test]
    fn a_handled_failure_is_told_by_its_place_once() {
        let at = Location::caller();
        let p = Report::handled("couldn't save an item's end", at).properties();
        let e = &p["$exception_list"][0];
        assert_eq!((e["type"].as_str(), e["value"].as_str()), (Some("HostError"), Some("couldn't save an item's end")));
        assert_eq!(e["mechanism"], json!({"type": "handled", "handled": true, "synthetic": false}));
        let frame = &e["stacktrace"]["frames"][0];
        assert_eq!((frame["filename"].as_str(), frame["lineno"].as_u64()), (Some(file!()), Some(u64::from(at.line()))));
        assert_eq!((p["process"].as_str(), p["$exception_level"].as_str()), (Some("host"), Some("error")));
        assert!(first_time(at) && !first_time(at), "a place reports once while the host runs");
    }

    #[test]
    fn a_logged_failure_is_reported_from_where_it_was_logged() {
        let hub = crate::chat::calls::context::tests::hub();
        let line = line!() + 1;
        host_error!(&hub, "couldn't hand work to a task", task = "t1");
        let mut rx = hub.analytics.rx.locked().take().expect("nothing read it yet");
        let Ok(Msg::Report(report)) = rx.try_recv() else { panic!("nothing was reported") };
        let at = &report.frames[0];
        assert_eq!(
            (report.value.as_str(), at.file.as_deref(), at.line),
            ("couldn't hand work to a task", Some(file!()), Some(line))
        );
    }

    #[test]
    fn only_the_host_reports_its_own_panics() {
        assert!(Report::deserialize(&json!({"process": "host", "type": "Panic", "mechanism": "panic"})).is_ok());
        assert!(Report::deserialize(&json!({"process": "phone", "type": "Panic", "mechanism": "panic"})).is_err());
    }

    #[test]
    fn a_panic_keeps_its_site_and_no_runtime_data() {
        let caught = Arc::new(Mutex::new(Vec::new()));
        let sink = caught.clone();
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if let Ok(mut c) = sink.lock() {
                c.push(Report::panic(info, Process::Tui));
            }
        }));
        let secret = "/Users/kevin/secret.txt";
        let _ = std::panic::catch_unwind(|| panic!("literal"));
        let _ = std::panic::catch_unwind(|| panic!("formatted {secret}"));
        let (len, index) = (1, 2);
        let _ = std::panic::catch_unwind(|| panic!("index out of bounds: the len is {len} but the index is {index}"));
        std::panic::set_hook(previous);
        let mut reports = std::mem::take(&mut *caught.lock().expect("not poisoned"));
        // Other tests panic in parallel; only this file's panics are ours.
        reports.retain(|r| r.frames.first().and_then(|f| f.file.as_deref()) == Some(file!()));
        assert_eq!(reports.len(), 3);
        assert_eq!(reports[0].value, "literal");
        assert_eq!(reports[1].value, "", "a formatted message can carry runtime data");
        assert_eq!(reports[2].value, "index out of bounds");
        let p = reports.into_iter().nth(1).expect("three reports").properties();
        let frame = &p["$exception_list"][0]["stacktrace"]["frames"][0];
        assert_eq!(frame["filename"], file!());
        assert_eq!((frame["lang"].as_str(), frame["in_app"].as_bool()), (Some("rust"), Some(true)));
        assert_eq!(p["$exception_level"], "fatal");
    }
}
