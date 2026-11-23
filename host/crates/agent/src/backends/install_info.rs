//! Where an agent on this computer comes from, for its sheet: the CLI found on it (its path, its version, and whether
//! Codync added it), and the ACP registry build Codync runs it through when the CLI has no ACP mode of its own (its
//! version, and whether it is downloaded yet). Read on demand for one agent: the CLI's `--version` is never run for
//! the whole list.

use super::{harness_of, installed_cli, run_cli};
use crate::registry;
use serde_json::{Value, json};

/// `{ cli, build, runs }` for `backend`. `cli`: `{ path, version, addedBy }` or null; `build`: `{ id, version, launch,
/// downloaded }` or null; `runs`: `"cli"` (its own ACP mode), `"build"` or null (not set up yet).
pub async fn install_info(backend: &str) -> Value {
    let h = harness_of(backend);
    let path = h.and_then(installed_cli);
    let version = match (h, &path) {
        (Some(h), Some(_)) => {
            run_cli(h, "--version", &[]).await.and_then(|out| version_of(&String::from_utf8_lossy(&out.stdout)))
        }
        _ => None,
    };
    let own_acp = path.is_some() && h.is_some_and(|h| h.local.is_some());
    let cli = path.map(|p| {
        let added_by = if p.starts_with(&crate::setup::get().data_dir) { "codync" } else { "computer" };
        json!({"path": p.to_string_lossy(), "version": version, "addedBy": added_by})
    });
    let build = match h.map_or(Some(backend), |h| h.registry).and_then(registry::agent).filter(|_| !own_acp) {
        Some(agent) => {
            let launch = registry::launch_kind(&agent).map(|l| match l {
                registry::Launch::Download => "download",
                registry::Launch::Npx => "npx",
                registry::Launch::Uvx => "uvx",
            });
            json!({"id": agent["id"], "version": agent["version"], "launch": launch, "downloaded": registry::downloaded(&agent).await})
        }
        None => Value::Null,
    };
    let runs = if own_acp {
        json!("cli")
    } else if build.is_null() {
        Value::Null
    } else {
        json!("build")
    };
    json!({"cli": cli, "build": build, "runs": runs})
}

/// The version a CLI's `--version` printed: its first word that starts with a digit and has a dot, without a `v`.
fn version_of(out: &str) -> Option<String> {
    out.lines().find_map(|line| {
        line.split(|c: char| c.is_whitespace() || matches!(c, ',' | '(' | ')' | '/'))
            .map(|w| w.strip_prefix('v').unwrap_or(w).trim_end_matches('.'))
            .find(|w| w.starts_with(|c: char| c.is_ascii_digit()) && w.contains('.'))
            .map(|w| w.chars().take(40).collect())
    })
}

#[cfg(test)]
mod tests {
    use super::version_of;

    #[test]
    fn reads_the_version_a_cli_prints() {
        assert_eq!(version_of("2.1.3 (Claude Code)\n").as_deref(), Some("2.1.3"));
        assert_eq!(version_of("codex-cli 0.58.0").as_deref(), Some("0.58.0"));
        assert_eq!(version_of("goose v1.12.0-beta.1").as_deref(), Some("1.12.0-beta.1"));
        assert_eq!(version_of("Usage: tool [options]\nversion 3.4"), Some("3.4".to_owned()));
        assert_eq!(version_of("GitHub Copilot CLI 1.0.114.").as_deref(), Some("1.0.114"));
        assert_eq!(version_of("no version here"), None);
    }
}
