//! Codex plan windows from the newest local rollout file.

use serde_json::Value;
use std::path::{Path, PathBuf};

use super::Window;

/// Blocking: reads the newest Codex rollout file.
pub(super) fn collect() -> Option<Vec<Window>> {
    let path = newest_rollout(&dirs::home_dir()?.join(".codex/sessions"))?;
    let text = std::fs::read_to_string(&path).ok()?;
    let line = text.lines().rev().find(|l| l.contains("\"rate_limits\""))?;
    let v: Value = serde_json::from_str(line).ok()?;
    let rl = if v["payload"]["rate_limits"].is_object() { &v["payload"]["rate_limits"] } else { &v["rate_limits"] };
    let modified_ms = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
    let windows = parse_codex(rl, modified_ms);
    (!windows.is_empty()).then_some(windows)
}

/// Blocking. `sessions/YYYY/MM/DD/rollout-*.jsonl`: descend into the largest dir each level.
fn newest_rollout(root: &Path) -> Option<PathBuf> {
    let mut dir = root.to_path_buf();
    for _ in 0..3 {
        let mut subs: Vec<PathBuf> =
            std::fs::read_dir(&dir).ok()?.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.is_dir()).collect();
        subs.sort();
        dir = subs.pop()?;
    }
    std::fs::read_dir(&dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
}

pub fn parse_codex(rl: &Value, observed_ms: i64) -> Vec<Window> {
    ["primary", "secondary"]
        .into_iter()
        .filter_map(|k| {
            let w = &rl[k];
            let percent = w["used_percent"].as_f64()?;
            let minutes = w["window_minutes"].as_i64().unwrap_or(0);
            let label = match minutes {
                300 => "5-hour".to_owned(),
                10080 => "Weekly".to_owned(),
                m if m > 0 && m % 1440 == 0 => format!("{}-day", m / 1440),
                m if m > 0 => format!("{}-hour", m / 60),
                _ => k.to_owned(),
            };
            let resets = w["resets_at"].as_i64().map(|s| s.saturating_mul(1000)).or_else(|| {
                w["resets_in_seconds"].as_i64().map(|s| observed_ms.saturating_add(s.saturating_mul(1000)))
            });
            Some(Window::new(k, label, percent, resets))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_codex_rollout_limits() {
        let x = parse_codex(
            &json!({"primary": {"used_percent": 3.0, "window_minutes": 300, "resets_in_seconds": 10},
                    "secondary": {"used_percent": 40.0, "window_minutes": 10080, "resets_at": 100}}),
            1000,
        );
        assert_eq!(x[0].label, "5-hour");
        assert_eq!(x[0].resets_at, Some(11_000));
        assert_eq!(x[1].resets_at, Some(100_000));
    }
}
