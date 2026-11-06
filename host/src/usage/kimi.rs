//! Interactive `kimi` in a terminal: wait until it accepts input, type
//! `/usage`, and read the plan lines. The TUI needs a terminal, so this is a
//! PTY rather than a pipe. Parsing covers the three layouts the CLI has shipped.

use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use super::Window;

const WAIT: Duration = Duration::from_secs(20);
/// How long the screen stays unchanged after the first plan row before it is read.
const SETTLE: Duration = Duration::from_millis(600);

pub(super) fn collect() -> Option<Vec<Window>> {
    let bin = super::command::program("kimi")?;
    let dir = probe_dir()?;
    let screen = capture(&bin, &dir)?;
    let windows = parse(&screen, crate::store::now_ms());
    if windows.is_empty() {
        tracing::debug!("kimi /usage screen had no plan lines");
        None
    } else {
        Some(windows)
    }
}

fn probe_dir() -> Option<std::path::PathBuf> {
    let dir = dirs::home_dir()?.join(".codync/probe");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// One `/usage` screen, or nothing if the CLI never settles.
fn capture(bin: &Path, dir: &Path) -> Option<String> {
    let pair = native_pty_system().openpty(PtySize { rows: 40, cols: 120, pixel_width: 0, pixel_height: 0 }).ok()?;
    let mut cmd = CommandBuilder::new(bin);
    cmd.cwd(dir);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    // An inherited key would answer for a different login than `kimi login`.
    cmd.env_remove("KIMI_AUTH_TOKEN");
    cmd.env_remove("KIMI_API_KEY");
    cmd.env_remove("KIMI_BASE_URL");
    let mut child = pair.slave.spawn_command(cmd).ok()?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().ok()?;
    let mut writer = pair.master.take_writer().ok()?;
    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buf = [0_u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });
    let started = Instant::now();
    let mut raw = Vec::new();
    let mut typed = 0_u8;
    let mut trusted = false;
    let mut answered = false;
    let mut changed = Instant::now();
    // Writes to the terminal are best effort: one that fails leaves the screen without a usage
    // panel, so the loop runs out at `WAIT` and the screen reads as no plan lines.
    while started.elapsed() < WAIT {
        while let Ok(chunk) = rx.try_recv() {
            raw.extend_from_slice(&chunk);
            changed = Instant::now();
        }
        if !answered && raw.windows(b"\x1b[6n".len()).any(|window| window == b"\x1b[6n") {
            let _ = writer.write_all(b"\x1b[2;1R");
            let _ = writer.flush();
            answered = true;
        }
        let screen = String::from_utf8_lossy(&raw);
        if !trusted && (screen.contains("Trust this folder?") || screen.contains("Yes, I trust")) {
            let _ = writer.write_all(b"\r");
            let _ = writer.flush();
            trusted = true;
        }
        let ready = screen.contains("context:") || screen.contains('💫');
        let settled = started.elapsed() >= Duration::from_millis(1500);
        if typed == 0 && settled && (ready || started.elapsed() >= Duration::from_secs(8)) {
            let _ = writer.write_all(b"/usage\r");
            let _ = writer.flush();
            typed = 1;
        } else if typed == 1 && started.elapsed() >= Duration::from_secs(12) && !done(&screen) {
            let _ = writer.write_all(b"/usage\r");
            let _ = writer.flush();
            typed = 2;
        }
        // The plan rows can come over several writes: stop once the screen has stayed still a moment.
        if typed > 0 && done(&screen) && changed.elapsed() >= SETTLE {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
    let screen = super::command::strip_ansi(&String::from_utf8_lossy(&raw));
    (!screen.trim().is_empty()).then_some(screen)
}

fn done(screen: &str) -> bool {
    screen.contains("% used")
        || screen.contains("% left")
        || screen.contains("Authorization failed")
        || screen.contains("not logged in")
}

fn parse(text: &str, now_ms: i64) -> Vec<Window> {
    let mut seen = std::collections::HashSet::new();
    let mut windows = Vec::new();
    for line in text.lines() {
        let Some((id, label)) = kind(line) else { continue };
        if !seen.insert(id) {
            continue;
        }
        let Some(percent) = percent_used(line) else { continue };
        let resets = resets_in(line).map(|ms| now_ms.saturating_add(ms));
        windows.push(Window::new(id, label, percent, resets));
    }
    windows
}

fn kind(line: &str) -> Option<(&'static str, &'static str)> {
    let lower = line.to_ascii_lowercase();
    if lower.contains("weekly") {
        Some(("seven_day", "Weekly"))
    } else if lower.contains("monthly") {
        Some(("monthly", "Monthly"))
    } else if lower.contains("5h") || lower.contains("hour") {
        Some(("five_hour", "5-hour"))
    } else {
        None
    }
}

fn percent_used(line: &str) -> Option<f64> {
    if let Some(n) = before_suffix(line, "% used") {
        return Some(n.clamp(0.0, 100.0));
    }
    before_suffix(line, "% left").map(|n| (100.0 - n).clamp(0.0, 100.0))
}

fn before_suffix(line: &str, suffix: &str) -> Option<f64> {
    let at = line.find(suffix)?;
    let head = line[..at].trim_end();
    let token: String = head.chars().rev().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    token.chars().rev().collect::<String>().parse().ok()
}

/// `resets in 6d 23h 22m` → milliseconds from now.
fn resets_in(line: &str) -> Option<i64> {
    let rest = line.split("resets in ").nth(1)?;
    let mut total = 0_i64;
    let mut number = String::new();
    let mut any = false;
    for c in rest.chars() {
        if c.is_ascii_digit() {
            number.push(c);
            continue;
        }
        if let Some(unit) = unit_ms(c)
            && let Ok(n) = number.parse::<i64>()
        {
            total = total.saturating_add(n.saturating_mul(unit));
            any = true;
        }
        number.clear();
        if c == ')' || c == '\n' {
            break;
        }
    }
    any.then_some(total)
}

fn unit_ms(c: char) -> Option<i64> {
    Some(match c {
        'd' => 86_400_000,
        'h' => 3_600_000,
        'm' => 60_000,
        's' => 1_000,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_layouts_become_used_percent_and_a_reset() {
        let old = "Weekly limit  100% left  (resets in 6d 23h)\n5h limit  80% left  (resets in 4h 22m)";
        let mid = "Weekly limit  90% used  resets in 35m\n5h limit  12% used  resets in 3h 35m";
        let new =
            "5h limit  0% used  resets in 2h 41m\nMonthly limit  2% used  resets in 24d 16h 42m\nkimi 2% · code 0%";
        let now = 1_000_000_i64;
        let a = parse(old, now);
        assert_eq!(a[0].id, "seven_day");
        assert!(a[0].percent.abs() < 1e-9);
        assert_eq!(a[0].resets_at, Some(now + 6 * 86_400_000 + 23 * 3_600_000));
        assert!((a[1].percent - 20.0).abs() < 1e-9);
        let b = parse(mid, now);
        assert!((b[0].percent - 90.0).abs() < 1e-9);
        assert_eq!(b[0].resets_at, Some(now + 35 * 60_000));
        let c = parse(new, now);
        assert_eq!(c[0].id, "five_hour");
        assert_eq!(c[1].label, "Monthly");
        assert!((c[1].percent - 2.0).abs() < 1e-9);
        assert_eq!(c.len(), 2, "the context mix line is not a plan window");
    }
}
