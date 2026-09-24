//! Platform glue: data dir, background service install (launchd / systemd),
//! keeping the machine awake during turns, and pairing addresses.

use anyhow::{Context, Result, bail};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

pub const DEFAULT_PORT: u16 = 19222;
const LABEL: &str = "com.pokai.codync.host";

pub fn data_dir() -> PathBuf {
    std::env::var_os("CODYNC_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().expect("no home dir").join(".codync"))
}

fn launchd_plist() -> PathBuf {
    dirs::home_dir().unwrap().join(format!("Library/LaunchAgents/{LABEL}.plist"))
}

fn systemd_unit() -> PathBuf {
    dirs::config_dir().unwrap().join("systemd/user/codync-host.service")
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Installs and starts the host as a per-user background service. The current
/// PATH is captured so the service finds `npx`, `claude`, `codex`, …
pub fn install(port: u16) -> Result<()> {
    let exe = std::env::current_exe()?.canonicalize()?;
    let exe = exe.to_string_lossy();
    let path = std::env::var("PATH").unwrap_or_default();
    let log = data_dir().join("host.log");
    std::fs::create_dir_all(data_dir())?;
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
        std::fs::create_dir_all(file.parent().unwrap())?;
        let uid = unsafe_uid();
        let _ = Command::new("launchctl").args(["bootout", &format!("gui/{uid}/{LABEL}")]).status();
        std::fs::write(&file, plist)?;
        let ok = Command::new("launchctl")
            .args(["bootstrap", &format!("gui/{uid}"), &file.to_string_lossy()])
            .status()?
            .success();
        if !ok {
            bail!("launchctl bootstrap failed");
        }
    } else {
        let unit = format!(
            "[Unit]\nDescription=Codync host\nAfter=network-online.target\n\n[Service]\nExecStart={exe} serve --port {port}\nEnvironment=PATH={path}\nRestart=always\nRestartSec=3\n\n[Install]\nWantedBy=default.target\n"
        );
        let file = systemd_unit();
        std::fs::create_dir_all(file.parent().unwrap())?;
        std::fs::write(&file, unit)?;
        run("systemctl", &["--user", "daemon-reload"])?;
        run("systemctl", &["--user", "enable", "--now", "codync-host.service"])?;
        println!("Tip: `loginctl enable-linger $USER` keeps the host running while you're logged out.");
    }
    Ok(())
}

pub fn uninstall() -> Result<()> {
    if cfg!(target_os = "macos") {
        let _ = Command::new("launchctl").args(["bootout", &format!("gui/{}/{LABEL}", unsafe_uid())]).status();
        let _ = std::fs::remove_file(launchd_plist());
    } else {
        let _ = run("systemctl", &["--user", "disable", "--now", "codync-host.service"]);
        let _ = std::fs::remove_file(systemd_unit());
        let _ = run("systemctl", &["--user", "daemon-reload"]);
    }
    Ok(())
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

fn unsafe_uid() -> String {
    Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .unwrap_or_else(|| "501".into())
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
                        .args(["--what=sleep:idle", "--who=Codync", "--why=A bot is working", "--mode=block", "sleep", "infinity"])
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()
                };
                self.0 = child.ok();
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
            if o[0] == 100 && (64..128).contains(&o[1]) {
                tailscale.push(format!("http://{ip}:{port}"));
            } else if ip.is_private() {
                lan.push(format!("http://{ip}:{port}"));
            }
        }
    }
    if let Ok(out) = Command::new("tailscale").args(["status", "--json"]).output()
        && let Ok(v) = serde_json::from_slice::<serde_json::Value>(&out.stdout)
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

fn pct(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn pairing_url(name: &str, token: &str, urls: &[String]) -> String {
    format!(
        "codync://pair?name={}&token={}&urls={}",
        pct(name),
        pct(token),
        pct(&urls.join(","))
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn pairing_url_is_escaped() {
        let u = super::pairing_url("Kevin's Mac", "t0k", &["http://100.1.2.3:19222".into(), "http://a:1".into()]);
        assert_eq!(u, "codync://pair?name=Kevin%27s%20Mac&token=t0k&urls=http%3A%2F%2F100.1.2.3%3A19222%2Chttp%3A%2F%2Fa%3A1");
    }
}

/// Codync 1.x installed `~/.codync/notify.sh` as Claude Code hooks. 2.x drives
/// agents over ACP instead, so those hooks are dead weight; remove only them.
/// Returns how many hook commands were removed.
pub fn remove_legacy_hooks(settings: &std::path::Path) -> Result<usize> {
    let Ok(text) = std::fs::read_to_string(settings) else { return Ok(0) };
    let mut v: serde_json::Value = serde_json::from_str(&text).context("parsing Claude settings")?;
    let mut removed = 0;
    if let Some(hooks) = v.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        for groups in hooks.values_mut() {
            let Some(groups) = groups.as_array_mut() else { continue };
            for g in groups.iter_mut() {
                if let Some(list) = g.get_mut("hooks").and_then(|l| l.as_array_mut()) {
                    let before = list.len();
                    list.retain(|h| !h["command"].as_str().is_some_and(|c| c.ends_with(".codync/notify.sh")));
                    removed += before - list.len();
                }
            }
            groups.retain(|g| g["hooks"].as_array().is_none_or(|l| !l.is_empty()));
        }
        hooks.retain(|_, groups| groups.as_array().is_none_or(|g| !g.is_empty()));
    }
    if v.get("hooks").and_then(|h| h.as_object()).is_some_and(|h| h.is_empty()) {
        v.as_object_mut().unwrap().remove("hooks");
    }
    if removed > 0 {
        std::fs::copy(settings, settings.with_extension("json.codync-backup"))?;
        std::fs::write(settings, serde_json::to_string_pretty(&v)? + "\n")?;
    }
    Ok(removed)
}

#[cfg(test)]
mod legacy_tests {
    #[test]
    fn removes_only_codync_hooks() {
        let dir = std::env::temp_dir().join(format!("codync-legacy-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("settings.json");
        std::fs::write(
            &f,
            r#"{"model":"x","hooks":{"Stop":[{"hooks":[{"type":"command","command":"/Users/a/.codync/notify.sh"},{"type":"command","command":"say hi"}]}],"Notification":[{"hooks":[{"type":"command","command":"/Users/a/.codync/notify.sh"}]}]}}"#,
        )
        .unwrap();
        assert_eq!(super::remove_legacy_hooks(&f).unwrap(), 2);
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        assert_eq!(v["hooks"]["Stop"][0]["hooks"].as_array().unwrap().len(), 1);
        assert!(v["hooks"].get("Notification").is_none());
        assert_eq!(v["model"], "x");
        assert!(dir.join("settings.json.codync-backup").exists());
    }
}
