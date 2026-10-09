//! The background service on macOS (a launchd agent) and Linux (a systemd user unit).

use super::{create_parent, current_exe, data_dir, home, run, wait_for_host_exit};
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const LABEL: &str = crate::environment::Environment::current().service_label();
const UNIT: &str = crate::environment::Environment::current().systemd_unit();

/// Stops the installed job, preserving its configuration for a later start.
/// A manually started daemon is never mistaken for the service we own.
pub fn stop() -> Result<()> {
    if cfg!(target_os = "macos") {
        let _ = Command::new("launchctl")
            .args(["bootout", &format!("gui/{}/{LABEL}", current_uid())])
            .stderr(Stdio::null())
            .status()?;
    } else if installed() {
        run("systemctl", &["--user", "stop", UNIT])?;
    }
    wait_for_host_exit()
}

pub fn start() -> Result<()> {
    if cfg!(target_os = "macos") {
        run("launchctl", &["bootstrap", &format!("gui/{}", current_uid()), &launchd_plist().to_string_lossy()])
    } else {
        run("systemctl", &["--user", "start", UNIT])
    }
}

/// Refuse to replace one installation while restarting a service owned by another.
pub fn require_executable(expected: &Path) -> Result<()> {
    let configured = if cfg!(target_os = "macos") {
        let output = Command::new("/usr/bin/plutil")
            .args(["-extract", "ProgramArguments.0", "raw", "-o", "-"])
            .arg(launchd_plist())
            .output()?;
        if !output.status.success() {
            bail!("cannot read the installed host service");
        }
        PathBuf::from(String::from_utf8(output.stdout)?.trim())
    } else {
        let unit = std::fs::read_to_string(systemd_unit())?;
        let command =
            unit.lines().find_map(|line| line.strip_prefix("ExecStart=")).context("service has no ExecStart")?;
        let args = shlex::split(command).context("invalid service ExecStart")?;
        PathBuf::from(args.first().context("service has no executable")?)
    };
    if configured.canonicalize()? != expected.canonicalize()? {
        bail!("the installed service runs another host binary; run that binary's update command");
    }
    Ok(())
}

fn launchd_plist() -> PathBuf {
    home().join(format!("Library/LaunchAgents/{LABEL}.plist"))
}

fn systemd_unit() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| home().join(".config")).join("systemd/user").join(UNIT)
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Installs and starts the host as a per-user background service. The current
/// PATH is captured so the service finds `npx`, `claude`, `codex`, …
pub fn install(port: u16) -> Result<()> {
    let exe = current_exe().context("locating the codync-host binary")?;
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
  <key>EnvironmentVariables</key><dict><key>PATH</key><string>{path}</string><key>CODYNC_HOME</key><string>{data}</string></dict>
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
            data = xml_escape(&data_dir().to_string_lossy()),
            log = xml_escape(&log.to_string_lossy()),
        );
        let file = launchd_plist();
        create_parent(&file)?;
        let uid = current_uid();
        // Not loaded yet is the normal case here.
        let _ =
            Command::new("launchctl").args(["bootout", &format!("gui/{uid}/{LABEL}")]).stderr(Stdio::null()).status();
        wait_for_host_exit()?;
        std::fs::write(&file, plist).with_context(|| format!("writing {}", file.display()))?;
        run("launchctl", &["bootstrap", &format!("gui/{uid}"), &file.to_string_lossy()])?;
    } else {
        let unit = format!(
            "[Unit]\nDescription=Codync host\nAfter=network-online.target\n\n[Service]\nExecStart={exe} serve --port {port}\nEnvironment=PATH={path}\nEnvironment=\"CODYNC_HOME={data}\"\nRestart=always\nRestartSec=3\n\n[Install]\nWantedBy=default.target\n",
            data = data_dir().display(),
        );
        let file = systemd_unit();
        create_parent(&file)?;
        std::fs::write(&file, unit).with_context(|| format!("writing {}", file.display()))?;
        run("systemctl", &["--user", "daemon-reload"])?;
        run("systemctl", &["--user", "stop", UNIT])?;
        wait_for_host_exit()?;
        run("systemctl", &["--user", "enable", "--now", UNIT])?;
        println!("Tip: `loginctl enable-linger $USER` keeps the host running while you're logged out.");
    }
    Ok(())
}

/// Best effort: every step tolerates "already gone".
pub fn uninstall() {
    if cfg!(target_os = "macos") {
        let _ = Command::new("launchctl")
            .args(["bootout", &format!("gui/{}/{LABEL}", current_uid())])
            .stderr(Stdio::null())
            .status();
        let _ = std::fs::remove_file(launchd_plist());
    } else {
        let _ = run("systemctl", &["--user", "disable", "--now", UNIT]);
        let _ = std::fs::remove_file(systemd_unit());
        let _ = run("systemctl", &["--user", "daemon-reload"]);
    }
}

pub fn installed() -> bool {
    if cfg!(target_os = "macos") { launchd_plist().exists() } else { systemd_unit().exists() }
}

fn current_uid() -> String {
    Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map_or_else(|| "501".into(), |s| s.trim().to_owned())
}
