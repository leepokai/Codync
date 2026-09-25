//! Which coding harnesses this machine has, and how to drive each over ACP.
//!
//! Detection works like Orca's preflight: hydrate PATH from the user's login
//! shell (GUI apps and launchd get a bare PATH), add the usual install dirs
//! (Homebrew, ~/.local/bin, nvm, bun, volta, asdf, mise, pnpm…), then look for
//! each known CLI. Launching prefers the installed CLI's own ACP mode and falls
//! back to the official ACP registry build (npx / uvx / downloaded binary).

use crate::LockExt;
use crate::registry;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

pub struct Harness {
    pub id: &'static str,
    pub name: &'static str,
    /// CLI names that mean "installed on this machine".
    pub bins: &'static [&'static str],
    /// The installed CLI's native ACP mode (`{bin}` = resolved binary path).
    pub local: Option<&'static str>,
    /// Entry in the ACP registry (adapter or pinned build).
    pub registry: Option<&'static str>,
    pub setup: &'static str,
}

pub const HARNESSES: &[Harness] = &[
    Harness {
        id: "claude",
        name: "Claude Code",
        bins: &["claude"],
        local: None,
        registry: Some("claude-acp"),
        setup: "Install Claude Code and run `claude` once to sign in.",
    },
    Harness {
        id: "codex",
        name: "Codex",
        bins: &["codex"],
        local: None,
        registry: Some("codex-acp"),
        setup: "Install Codex CLI and run `codex login`.",
    },
    Harness {
        id: "cursor",
        name: "Cursor",
        bins: &["cursor-agent"],
        local: Some("{bin} acp"),
        registry: Some("cursor"),
        setup: "Install Cursor CLI (curl https://cursor.com/install -fsS | bash) and run `cursor-agent login`.",
    },
    Harness {
        id: "pi",
        name: "Pi",
        bins: &["pi"],
        local: None,
        registry: Some("pi-acp"),
        setup: "Install pi (npm install -g @mariozechner/pi-coding-agent) and sign in with `pi`.",
    },
    Harness {
        id: "opencode",
        name: "OpenCode",
        bins: &["opencode"],
        local: Some("{bin} acp"),
        registry: Some("opencode"),
        setup: "Install OpenCode (curl -fsSL https://opencode.ai/install | bash) and run `opencode auth login`.",
    },
    Harness {
        id: "grok",
        name: "Grok Build",
        bins: &["grok"],
        local: Some("{bin} agent stdio"),
        registry: Some("grok-build"),
        setup: "Install Grok Build (curl -fsSL https://x.ai/cli/install.sh | bash) and run `grok` once to sign in.",
    },
    Harness {
        id: "gemini",
        name: "Gemini CLI",
        bins: &["gemini"],
        local: Some("{bin} --acp"),
        registry: Some("gemini"),
        setup: "Install Gemini CLI (npm install -g @google/gemini-cli) and run `gemini` once to sign in.",
    },
    Harness {
        id: "copilot",
        name: "GitHub Copilot",
        bins: &["copilot"],
        local: Some("{bin} --acp --stdio"),
        registry: Some("github-copilot-cli"),
        setup: "Install Copilot CLI (npm install -g @github/copilot) and run `copilot` to sign in.",
    },
    Harness {
        id: "qwen",
        name: "Qwen Code",
        bins: &["qwen"],
        local: Some("{bin} --acp"),
        registry: Some("qwen-code"),
        setup: "Install Qwen Code (npm install -g @qwen-code/qwen-code) and sign in with `qwen`.",
    },
    Harness {
        id: "goose",
        name: "goose",
        bins: &["goose"],
        local: Some("{bin} acp"),
        registry: Some("goose"),
        setup: "Install goose and run `goose configure`.",
    },
    Harness {
        id: "kimi",
        name: "Kimi CLI",
        bins: &["kimi"],
        local: Some("{bin} acp"),
        registry: Some("kimi"),
        setup: "Install Kimi CLI and sign in with `kimi`.",
    },
    Harness {
        id: "droid",
        name: "Factory Droid",
        bins: &["droid"],
        local: Some("{bin} exec --output-format acp-daemon"),
        registry: Some("factory-droid"),
        setup: "Install Droid (curl -fsSL https://app.factory.ai/cli | sh) and sign in with `droid`.",
    },
    Harness {
        id: "amp",
        name: "Amp",
        bins: &["amp"],
        local: None,
        registry: Some("amp-acp"),
        setup: "Install Amp and sign in with `amp login`.",
    },
    Harness {
        id: "kilo",
        name: "Kilo",
        bins: &["kilo"],
        local: Some("{bin} acp"),
        registry: Some("kilo"),
        setup: "Install Kilo CLI (npm install -g @kilocode/cli) and sign in.",
    },
    Harness {
        id: "cline",
        name: "Cline",
        bins: &["cline"],
        local: Some("{bin} --acp"),
        registry: Some("cline"),
        setup: "Install Cline CLI (npm install -g cline) and sign in.",
    },
    Harness {
        id: "auggie",
        name: "Auggie",
        bins: &["auggie"],
        local: Some("{bin} --acp"),
        registry: Some("auggie"),
        setup: "Install Auggie (npm install -g @augmentcode/auggie) and run `auggie login`.",
    },
    Harness {
        id: "vibe",
        name: "Mistral Vibe",
        bins: &["vibe-acp", "vibe"],
        local: None,
        registry: Some("mistral-vibe"),
        setup: "Install Mistral Vibe and run `vibe` once to sign in.",
    },
    Harness {
        id: "kiro",
        name: "Kiro CLI",
        bins: &["kiro-cli"],
        local: Some("{bin} acp"),
        registry: None,
        setup: "Install Kiro CLI (curl -fsSL https://cli.kiro.dev/install | bash) and run `kiro-cli login`.",
    },
    Harness {
        id: "devin",
        name: "Devin",
        bins: &["devin"],
        local: Some("{bin} acp"),
        registry: Some("devin"),
        setup: "Install the Devin CLI and sign in.",
    },
    Harness {
        id: "qoder",
        name: "Qoder CLI",
        bins: &["qodercli"],
        local: Some("{bin} --acp"),
        registry: Some("qoder"),
        setup: "Install Qoder CLI and sign in.",
    },
];

pub fn harness(id: &str) -> Option<&'static Harness> {
    HARNESSES.iter().find(|h| h.id == id)
}

// MARK: PATH hydration

static SEARCH_PATH: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Login-shell PATH, via `$SHELL -ilc` with a timeout (nvm/conda can make shells slow).
fn login_shell_path() -> Option<String> {
    const MARK: &str = "__CODYNC_PATH__";
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let mut child = std::process::Command::new(shell)
        .args(["-ilc", &format!("printf '%s%s%s' '{MARK}' \"$PATH\" '{MARK}'")])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                return None;
            }
        }
    }
    let mut out = String::new();
    std::io::Read::read_to_string(&mut child.stdout.take()?, &mut out).ok()?;
    let start = out.find(MARK)? + MARK.len();
    let end = start + out[start..].find(MARK)?;
    Some(out[start..end].to_owned())
}

/// `…/v22.1.0` → `[22, 1, 0]` (unparseable parts count as 0).
fn node_version(dir: &Path) -> Vec<u32> {
    dir.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.trim_start_matches('v').split('.').map(|p| p.parse().unwrap_or(0)).collect())
        .unwrap_or_default()
}

fn well_known_dirs() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else { return vec![] };
    let mut dirs: Vec<PathBuf> =
        ["/opt/homebrew/bin", "/usr/local/bin", "/home/linuxbrew/.linuxbrew/bin", "/usr/bin", "/bin"]
            .iter()
            .map(PathBuf::from)
            .collect();
    for rel in [
        ".local/bin",
        ".bun/bin",
        ".cargo/bin",
        ".volta/bin",
        ".asdf/shims",
        ".local/share/mise/shims",
        ".fnm/aliases/default/bin",
        ".local/share/pnpm",
        "Library/pnpm",
        ".npm-global/bin",
        ".opencode/bin",
        ".deno/bin",
        "go/bin",
    ] {
        dirs.push(home.join(rel));
    }
    // Every nvm-installed node, newest first (global npm CLIs live next to node).
    if let Ok(entries) = std::fs::read_dir(home.join(".nvm/versions/node")) {
        let mut versions: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
        // Numeric, not lexicographic: v22.1.0 is newer than v9.11.2.
        versions.sort_by_key(|p| std::cmp::Reverse(node_version(p)));
        dirs.extend(versions.into_iter().map(|p| p.join("bin")));
    }
    dirs
}

/// Re-detects the search path and exports it as this process's PATH, so every
/// agent we spawn (and the `npx`/`node` they need) resolves the same way.
pub fn hydrate_path() {
    let mut seen = HashSet::new();
    let mut path: Vec<PathBuf> = vec![];
    let current = std::env::var("PATH").unwrap_or_default();
    let login = login_shell_path().unwrap_or_default();
    for p in std::env::split_paths(&current).chain(std::env::split_paths(&login)).chain(well_known_dirs()) {
        if p.as_os_str().is_empty() || !p.is_dir() {
            continue;
        }
        if seen.insert(p.clone()) {
            path.push(p);
        }
    }
    if let Ok(joined) = std::env::join_paths(&path) {
        // SAFETY: called at startup and from the single refresh command, before/between spawns.
        unsafe { std::env::set_var("PATH", joined) };
    }
    *SEARCH_PATH.locked() = path;
}

pub fn which(bin: &str) -> Option<PathBuf> {
    let dirs = {
        let cached = SEARCH_PATH.locked();
        if cached.is_empty() {
            std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default()
        } else {
            cached.clone()
        }
    };
    dirs.into_iter().map(|d| d.join(bin)).find(|p| is_executable(p))
}

fn is_executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        p.is_file()
    }
}

pub fn on_path(bin: &str) -> bool {
    which(bin).is_some()
}

// MARK: listing + launching

/// Every harness the phone can pick, installed ones first.
pub fn list() -> Vec<Value> {
    let registry = registry::agents();
    let mut out: Vec<Value> = HARNESSES
        .iter()
        .map(|h| {
            let path = h.bins.iter().find_map(|b| which(b));
            let reg = h.registry.and_then(|id| registry.iter().find(|a| a["id"] == id));
            let runnable =
                (path.is_some() && h.local.is_some()) || reg.is_some_and(|a| registry::launch_kind(a).is_some());
            json!({
                "id": h.id,
                "name": h.name,
                "installed": path.is_some(),
                "available": path.is_some() && runnable,
                "path": path.map(|p| p.to_string_lossy().into_owned()),
                "description": reg.and_then(|a| a["description"].as_str()).unwrap_or_default(),
                "installHint": h.setup,
                "command": h.local.unwrap_or_default(),
                "registry": h.registry,
                "curated": true,
            })
        })
        .collect();
    // Registry agents we don't know by name: runnable, but you sign in yourself.
    let known: HashSet<&str> = HARNESSES.iter().filter_map(|h| h.registry).collect();
    for a in &registry {
        let id = a["id"].as_str().unwrap_or_default();
        if id.is_empty() || known.contains(id) || id == "github-copilot" {
            continue;
        }
        out.push(json!({
            "id": id,
            "name": a["name"],
            "installed": false,
            "available": registry::launch_kind(a).is_some(),
            "path": null,
            "description": a["description"],
            "installHint": format!("Codync installs it automatically. Sign in to {} on this computer first if it needs an account.", a["name"].as_str().unwrap_or(id)),
            "command": "",
            "registry": id,
            "curated": false,
        }));
    }
    out.sort_by_key(|v| (!v["installed"].as_bool().unwrap_or(false), !v["curated"].as_bool().unwrap_or(false)));
    out
}

pub fn is_known(id: &str) -> bool {
    harness(id).is_some() || registry::agent(id).is_some()
}

/// Ways to start this backend, best first. Local CLIs can be too old to speak
/// ACP, so the registry build is kept as a fallback.
pub async fn launch_candidates(id: &str, progress: impl Fn(&str)) -> anyhow::Result<Vec<String>> {
    let mut out = vec![];
    let h = harness(id);
    if let Some(h) = h
        && let Some(template) = h.local
        && let Some(path) = h.bins.iter().find_map(|b| which(b))
    {
        out.push(template.replace("{bin}", &shell_quote(&path.to_string_lossy())));
    }
    let reg_id = h.map_or(Some(id), |h| h.registry);
    if let Some(agent) = reg_id.and_then(registry::agent) {
        match registry::command(&agent, &progress).await {
            Ok(cmd) => out.push(cmd),
            Err(e) if out.is_empty() => return Err(e),
            Err(e) => tracing::info!(backend = id, error = format!("{e:#}"), "registry build unavailable"),
        }
    }
    if out.is_empty() {
        let hint = h.map_or("Check that it's installed on this computer.", |h| h.setup);
        anyhow::bail!("{} isn't set up on this computer. {hint}", h.map_or(id, |h| h.name));
    }
    Ok(out)
}

fn shell_quote(s: &str) -> String {
    if s.chars().all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c)) {
        s.to_owned()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_executables_on_search_path() {
        let dir = std::env::temp_dir().join(format!("codync-bin-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let bin = dir.join("fake-harness");
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        *SEARCH_PATH.locked() = vec![dir.clone()];
        assert!(which("fake-harness").is_none(), "not executable yet");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert_eq!(which("fake-harness"), Some(bin));
        SEARCH_PATH.locked().clear();
    }

    #[test]
    fn every_harness_is_unique() {
        let ids: HashSet<_> = HARNESSES.iter().map(|h| h.id).collect();
        assert_eq!(ids.len(), HARNESSES.len());
    }
}
