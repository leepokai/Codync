//! Platform glue: data dir, background service install (launchd / systemd),
//! keeping the machine awake during turns, pairing addresses, and the Claude
//! Code status line we route through the host.
//!
//! Everything here is blocking (`std::fs`, `std::process`): async callers go
//! through `spawn_blocking`.

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

pub const DEFAULT_PORT: u16 = 19222;
const LABEL: &str = "com.pokai.codync.host";
/// Marker between our command and a wrapped user status line.
const STATUSLINE_MARK: &str = " statusline --";

/// The user's home. Codync can't do anything useful without one, so its absence is fatal.
fn home() -> PathBuf {
    dirs::home_dir().expect("HOME must be set: Codync keeps its data and agent settings there")
}

pub fn data_dir() -> PathBuf {
    std::env::var_os("CODYNC_HOME").map_or_else(|| home().join(".codync"), PathBuf::from)
}

fn launchd_plist() -> PathBuf {
    home().join(format!("Library/LaunchAgents/{LABEL}.plist"))
}

fn systemd_unit() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| home().join(".config")).join("systemd/user/codync-host.service")
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn create_parent(file: &Path) -> Result<()> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    Ok(())
}

/// Installs and starts the host as a per-user background service. The current
/// PATH is captured so the service finds `npx`, `claude`, `codex`, …
pub fn install(port: u16) -> Result<()> {
    let exe = std::env::current_exe()?.canonicalize().context("locating the codync-host binary")?;
    let exe = exe.to_string_lossy();
    let path = std::env::var("PATH").unwrap_or_default();
    let log = data_dir().join("host.log");
    std::fs::create_dir_all(data_dir()).context("creating the data directory")?;
    if cfg!(target_os = "macos") {
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key><array><string>{exe}</string><string>serve</string><string>--port</string><string>{port}</string></array>
  <key>EnvironmentVariables</key><dict><key>PATH</key><string>{path}</string></dict>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>ProcessType</key><string>Interactive</string>
  <key>StandardOutPath</key><string>{log}</string>
  <key>StandardErrorPath</key><string>{log}</string>
</dict>
</plist>
"#,
            exe = xml_escape(&exe),
            path = xml_escape(&path),
            log = xml_escape(&log.to_string_lossy()),
        );
        let file = launchd_plist();
        create_parent(&file)?;
        let uid = current_uid();
        // Not loaded yet is the normal case here.
        let _ = Command::new("launchctl").args(["bootout", &format!("gui/{uid}/{LABEL}")]).status();
        std::fs::write(&file, plist).with_context(|| format!("writing {}", file.display()))?;
        run("launchctl", &["bootstrap", &format!("gui/{uid}"), &file.to_string_lossy()])?;
    } else {
        let unit = format!(
            "[Unit]\nDescription=Codync host\nAfter=network-online.target\n\n[Service]\nExecStart={exe} serve --port {port}\nEnvironment=PATH={path}\nRestart=always\nRestartSec=3\n\n[Install]\nWantedBy=default.target\n"
        );
        let file = systemd_unit();
        create_parent(&file)?;
        std::fs::write(&file, unit).with_context(|| format!("writing {}", file.display()))?;
        run("systemctl", &["--user", "daemon-reload"])?;
        run("systemctl", &["--user", "enable", "--now", "codync-host.service"])?;
        println!("Tip: `loginctl enable-linger $USER` keeps the host running while you're logged out.");
    }
    Ok(())
}

/// Best effort: every step tolerates "already gone".
pub fn uninstall() {
    if cfg!(target_os = "macos") {
        let _ = Command::new("launchctl").args(["bootout", &format!("gui/{}/{LABEL}", current_uid())]).status();
        let _ = std::fs::remove_file(launchd_plist());
    } else {
        let _ = run("systemctl", &["--user", "disable", "--now", "codync-host.service"]);
        let _ = std::fs::remove_file(systemd_unit());
        let _ = run("systemctl", &["--user", "daemon-reload"]);
    }
}

pub fn installed() -> bool {
    if cfg!(target_os = "macos") { launchd_plist().exists() } else { systemd_unit().exists() }
}

fn run(cmd: &str, args: &[&str]) -> Result<()> {
    let ok = Command::new(cmd).args(args).status().with_context(|| format!("running {cmd}"))?.success();
    if !ok {
        bail!("{cmd} {} failed", args.join(" "));
    }
    Ok(())
}

fn current_uid() -> String {
    Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map_or_else(|| "501".into(), |s| s.trim().to_owned())
}

/// Holds a sleep inhibitor while any bot is working.
#[derive(Default)]
pub struct KeepAwake(Option<Child>);

impl KeepAwake {
    pub fn set(&mut self, on: bool) {
        match (on, self.0.is_some()) {
            (true, false) => {
                let pid = std::process::id().to_string();
                let child = if cfg!(target_os = "macos") {
                    Command::new("caffeinate").args(["-i", "-w", &pid]).stdout(Stdio::null()).spawn()
                } else {
                    Command::new("systemd-inhibit")
                        .args([
                            "--what=sleep:idle",
                            "--who=Codync",
                            "--why=A bot is working",
                            "--mode=block",
                            "sleep",
                            "infinity",
                        ])
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()
                };
                self.0 = child.map_err(|error| tracing::warn!(%error, "can't keep the machine awake")).ok();
            }
            (false, true) => {
                if let Some(mut c) = self.0.take() {
                    let _ = c.kill();
                    let _ = c.wait();
                }
            }
            _ => {}
        }
    }
}

/// Addresses the phone can try, best first: Tailscale, then LAN.
pub fn addresses(port: u16) -> Vec<String> {
    let mut tailscale = vec![];
    let mut lan = vec![];
    for iface in if_addrs::get_if_addrs().unwrap_or_default() {
        if iface.is_loopback() {
            continue;
        }
        if let std::net::IpAddr::V4(ip) = iface.ip() {
            let o = ip.octets();
            // 100.64.0.0/10 is Tailscale's CGNAT range.
            if o[0] == 100 && (64..128).contains(&o[1]) {
                tailscale.push(format!("http://{ip}:{port}"));
            } else if ip.is_private() {
                lan.push(format!("http://{ip}:{port}"));
            }
        }
    }
    if let Ok(out) = Command::new("tailscale").args(["status", "--json"]).output()
        && let Ok(v) = serde_json::from_slice::<Value>(&out.stdout)
        && let Some(dns) = v["Self"]["DNSName"].as_str()
    {
        let dns = dns.trim_end_matches('.');
        if !dns.is_empty() {
            tailscale.insert(0, format!("http://{dns}:{port}"));
        }
    }
    tailscale.sort();
    tailscale.dedup();
    lan.sort();
    lan.dedup();
    tailscale.into_iter().chain(lan).collect()
}

pub fn host_name() -> String {
    let n = gethostname::gethostname().to_string_lossy().into_owned();
    n.trim_end_matches(".local").to_owned()
}

/// RFC 3986 percent-encoding of everything but unreserved characters.
fn pct(s: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

pub fn pairing_url(name: &str, token: &str, urls: &[String]) -> String {
    format!("codync://pair?name={}&token={}&urls={}", pct(name), pct(token), pct(&urls.join(",")))
}

fn read_settings(settings: &Path) -> Result<Option<Value>> {
    match std::fs::read_to_string(settings) {
        Ok(text) => Ok(Some(serde_json::from_str(&text).context("parsing Claude settings")?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {}", settings.display())),
    }
}

fn write_settings(settings: &Path, v: &Value) -> Result<()> {
    create_parent(settings)?;
    std::fs::write(settings, serde_json::to_string_pretty(v)? + "\n")
        .with_context(|| format!("writing {}", settings.display()))
}

/// Routes Claude Code's status line through `codync-host statusline` so usage
/// limits reach the host locally. An existing status line keeps working: it is
/// wrapped (`codync-host statusline -- <original>`) and restored on uninstall.
pub fn ensure_statusline(settings: &Path) -> Result<bool> {
    let mut v = read_settings(settings)?.unwrap_or_else(|| json!({}));
    let current = v["statusLine"]["command"].as_str().map(str::to_owned);
    if current.as_deref().is_some_and(|c| c.contains(" statusline")) {
        return Ok(false);
    }
    if v.get("statusLine").is_some() && current.is_none() {
        return Ok(false); // not a command status line; leave it alone
    }
    let exe = std::env::current_exe()?.canonicalize()?;
    let ours = format!("'{}' statusline", exe.to_string_lossy());
    let command = match current {
        Some(original) => format!("{ours} -- {original}"),
        None => ours,
    };
    v["statusLine"] = json!({"type": "command", "command": command});
    write_settings(settings, &v)?;
    Ok(true)
}

/// Undoes [`ensure_statusline`].
pub fn restore_statusline(settings: &Path) -> Result<()> {
    let Some(mut v) = read_settings(settings)? else { return Ok(()) };
    let Some(cmd) = v["statusLine"]["command"].as_str().map(str::to_owned) else { return Ok(()) };
    // Ours always starts with the quoted host path followed by ` statusline`.
    if !cmd.starts_with('\'') || !cmd.contains("' statusline") {
        return Ok(());
    }
    match cmd.split_once(STATUSLINE_MARK) {
        Some((_, original)) => v["statusLine"]["command"] = original.trim().into(),
        None => {
            if let Some(root) = v.as_object_mut() {
                root.remove("statusLine");
            }
        }
    }
    write_settings(settings, &v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_settings(contents: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("codync-settings-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("settings.json");
        std::fs::write(&f, contents).unwrap();
        f
    }

    fn read(f: &Path) -> Value {
        serde_json::from_str(&std::fs::read_to_string(f).unwrap()).unwrap()
    }

    #[test]
    fn pairing_url_is_escaped() {
        let u = pairing_url("Kevin's Mac", "t0k", &["http://100.1.2.3:19222".into(), "http://a:1".into()]);
        assert_eq!(
            u,
            "codync://pair?name=Kevin%27s%20Mac&token=t0k&urls=http%3A%2F%2F100.1.2.3%3A19222%2Chttp%3A%2F%2Fa%3A1"
        );
    }

    #[test]
    fn statusline_wraps_and_restores() {
        let f = temp_settings(r#"{"statusLine":{"type":"command","command":"sh ~/mine.sh"}}"#);
        assert!(ensure_statusline(&f).unwrap());
        assert!(!ensure_statusline(&f).unwrap(), "idempotent");
        assert!(read(&f)["statusLine"]["command"].as_str().unwrap().ends_with("statusline -- sh ~/mine.sh"));
        restore_statusline(&f).unwrap();
        assert_eq!(read(&f)["statusLine"]["command"], "sh ~/mine.sh");
    }
}
