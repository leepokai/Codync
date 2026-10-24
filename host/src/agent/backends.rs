//! Which coding harnesses this machine has, and how to drive each over ACP.
//!
//! Detection works like Orca's preflight: hydrate PATH from the user's login
//! shell (GUI apps and launchd get a bare PATH), add the usual install dirs
//! (Homebrew, ~/.local/bin, nvm, bun, volta, asdf, mise, pnpm…), then look for
//! each known CLI. Launching prefers the installed CLI's own ACP mode and falls
//! back to the official ACP registry build (npx / uvx / downloaded binary), which
//! is resolved only when it's tried: an installed CLI that starts never downloads
//! a second copy.

use crate::agent::{acp, node, registry};
use crate::shell;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::path::Path;

mod search_path;
mod sign_in;
pub use search_path::{hydrate_path, on_path, which};
pub use sign_in::{SignInCheck, cli_signed_in, refresh_sign_in, set_signed_in, signed_in};
use sign_in::{claude_signed_in, exit_ok, qoder_signed_in, says_logged_in};

pub struct Harness {
    pub id: &'static str,
    pub name: &'static str,
    /// CLI names that mean "installed on this machine".
    pub bins: &'static [&'static str],
    /// The installed CLI's native ACP mode (`{bin}` = resolved binary path).
    pub local: Option<&'static str>,
    /// Entry in the ACP registry (adapter or pinned build).
    pub registry: Option<&'static str>,
    /// What to tell someone setting it up by hand.
    pub setup: &'static str,
    /// How Codync installs the CLI for you (in a setup terminal).
    pub install: Option<Install>,
    /// Shell command that signs in, run in a setup terminal (`{bin}` = the CLI); `None` when only the
    /// agent's own ACP sign-in methods reach the session Codync runs.
    pub login: Option<&'static str>,
    /// How to tell whether it's signed in without starting it.
    pub signed_in: Option<SignInCheck>,
}

#[derive(Clone, Copy)]
pub enum Install {
    /// A vendor install script (`curl … | bash`), Unix only.
    Script(&'static str),
    /// Vendor install scripts for Unix and for Windows (PowerShell).
    Scripts { unix: &'static str, windows: &'static str },
    /// A global npm package, installed on the computer's Node or else Codync's (`codync-host npm`).
    Npm(&'static str),
}

impl Install {
    /// `None`: nothing Codync can run on this platform.
    pub fn command(self) -> Option<String> {
        match self {
            Self::Script(s) | Self::Scripts { unix: s, .. } if cfg!(unix) => Some(s.to_owned()),
            Self::Script(_) => None,
            Self::Scripts { windows, .. } => Some(windows.to_owned()),
            Self::Npm(pkg) => crate::service::current_exe()
                .ok()
                .map(|exe| format!("{} npm install -g {pkg}", shell::quote(&exe.to_string_lossy()))),
        }
    }
}

pub const HARNESSES: &[Harness] = &[
    Harness {
        id: "claude",
        name: "Claude Code",
        bins: &["claude"],
        local: None,
        registry: Some("claude-acp"),
        setup: "Install Claude Code and run `claude auth login`.",
        install: Some(Install::Scripts {
            unix: "curl -fsSL https://claude.ai/install.sh | bash",
            windows: "powershell -NoProfile -ExecutionPolicy Bypass -Command \"irm https://claude.ai/install.ps1 | iex\"",
        }),
        login: Some("{bin} auth login"),
        signed_in: Some(SignInCheck { args: "auth status", ok: claude_signed_in }),
    },
    Harness {
        id: "codex",
        name: "Codex",
        bins: &["codex"],
        local: None,
        registry: Some("codex-acp"),
        setup: "Install Codex CLI and run `codex login --device-auth`.",
        install: Some(Install::Scripts {
            unix: "curl -fsSL https://chatgpt.com/codex/install.sh | sh",
            windows: "powershell -NoProfile -ExecutionPolicy Bypass -Command \"irm https://chatgpt.com/codex/install.ps1 | iex\"",
        }),
        login: Some("{bin} login --device-auth"),
        signed_in: Some(SignInCheck { args: "login status", ok: exit_ok }),
    },
    Harness {
        id: "cursor",
        name: "Cursor",
        bins: &["cursor-agent", "agent"],
        local: Some("{bin} acp"),
        registry: Some("cursor"),
        setup: "Install Cursor CLI (curl https://cursor.com/install -fsS | bash) and run `cursor-agent login`.",
        install: Some(Install::Scripts {
            unix: "curl https://cursor.com/install -fsS | bash",
            windows: "powershell -NoProfile -ExecutionPolicy Bypass -Command \"irm 'https://cursor.com/install?win32=true' | iex\"",
        }),
        login: Some("NO_OPEN_BROWSER=1 {bin} login"),
        signed_in: Some(SignInCheck { args: "status", ok: says_logged_in }),
    },
    Harness {
        id: "pi",
        name: "Pi",
        bins: &["pi"],
        local: None,
        registry: Some("pi-acp"),
        setup: "Install pi (npm install -g @earendil-works/pi-coding-agent), run `pi` and type /login.",
        install: Some(Install::Npm("@earendil-works/pi-coding-agent")),
        login: Some("{bin}"),
        signed_in: None,
    },
    Harness {
        id: "omp",
        name: "oh-my-pi",
        bins: &["omp"],
        local: Some("{bin} acp"),
        registry: None,
        setup: "Install oh-my-pi (curl -fsSL https://omp.sh/install | sh -s -- --binary) and run `omp login`.",
        install: Some(Install::Scripts {
            // The prebuilt binary: by default the scripts build through any Bun on PATH and stop when it's older than they need.
            unix: "curl -fsSL https://omp.sh/install | sh -s -- --binary",
            windows: "powershell -NoProfile -ExecutionPolicy Bypass -Command \"& ([scriptblock]::Create((irm https://omp.sh/install.ps1))) -Binary\"",
        }),
        login: Some("{bin} login"),
        signed_in: None,
    },
    Harness {
        id: "opencode",
        name: "OpenCode",
        bins: &["opencode"],
        local: Some("{bin} acp"),
        registry: Some("opencode"),
        setup: "Install OpenCode (curl -fsSL https://opencode.ai/install | bash) and run `opencode auth login`.",
        install: Some(Install::Script("curl -fsSL https://opencode.ai/install | bash")),
        login: Some("{bin} auth login"),
        signed_in: None,
    },
    Harness {
        id: "grok",
        name: "Grok Build",
        bins: &["grok"],
        local: Some("{bin} agent stdio"),
        registry: Some("grok-build"),
        setup: "Install Grok Build (curl -fsSL https://x.ai/cli/install.sh | bash) and run `grok login --device-auth`.",
        install: Some(Install::Scripts {
            unix: "curl -fsSL https://x.ai/cli/install.sh | bash",
            windows: "powershell -NoProfile -ExecutionPolicy Bypass -Command \"irm https://x.ai/cli/install.ps1 | iex\"",
        }),
        login: Some("{bin} login --device-auth"),
        signed_in: None,
    },
    Harness {
        id: "gemini",
        name: "Gemini CLI",
        bins: &["gemini"],
        local: Some("{bin} --acp"),
        registry: Some("gemini"),
        setup: "Install Gemini CLI (npm install -g @google/gemini-cli) and run `gemini` to sign in with Google.",
        install: Some(Install::Npm("@google/gemini-cli")),
        login: Some("NO_BROWSER=true {bin}"),
        signed_in: None,
    },
    Harness {
        id: "copilot",
        name: "GitHub Copilot",
        bins: &["copilot"],
        local: Some("{bin} --acp --stdio"),
        registry: Some("github-copilot-cli"),
        setup: "Install Copilot CLI (curl -fsSL https://gh.io/copilot-install | bash) and run `copilot login`.",
        install: Some(Install::Scripts {
            unix: "curl -fsSL https://gh.io/copilot-install | bash",
            windows: "winget install --id GitHub.Copilot -e",
        }),
        login: Some("{bin} login --device-code"),
        signed_in: None,
    },
    Harness {
        id: "qwen",
        name: "Qwen Code",
        bins: &["qwen"],
        local: Some("{bin} --acp"),
        registry: Some("qwen-code"),
        setup: "Install Qwen Code (its standalone installer), run `qwen` and type /auth.",
        install: Some(Install::Scripts {
            unix: "curl -fsSL https://qwen-code-assets.oss-cn-hangzhou.aliyuncs.com/installation/install-qwen-standalone.sh | bash",
            windows: "powershell -NoProfile -ExecutionPolicy Bypass -Command \"irm https://qwen-code-assets.oss-cn-hangzhou.aliyuncs.com/installation/install-qwen-standalone.ps1 | iex\"",
        }),
        login: Some("{bin}"),
        signed_in: None,
    },
    Harness {
        id: "hermes",
        name: "Hermes Agent",
        bins: &["hermes"],
        local: Some("{bin} acp"),
        registry: None,
        setup: "Install Hermes Agent and run `hermes acp --setup` to choose a provider and model.",
        install: Some(Install::Scripts {
            unix: "curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash",
            windows: "powershell -NoProfile -ExecutionPolicy Bypass -Command \"iex (irm https://hermes-agent.nousresearch.com/install.ps1)\"",
        }),
        login: Some("{bin} acp --setup"),
        signed_in: None,
    },
    Harness {
        id: "goose",
        name: "goose",
        bins: &["goose"],
        local: Some("{bin} acp"),
        registry: Some("goose"),
        setup: "Install goose and run `goose configure`.",
        install: Some(Install::Script(
            "curl -fsSL https://github.com/aaif-goose/goose/releases/download/stable/download_cli.sh | bash",
        )),
        login: Some("{bin} configure"),
        signed_in: None,
    },
    Harness {
        id: "kimi",
        name: "Kimi Code",
        bins: &["kimi"],
        local: Some("{bin} acp"),
        registry: Some("kimi"),
        setup: "Install Kimi Code (curl -fsSL https://code.kimi.com/kimi-code/install.sh | bash) and run `kimi login`.",
        install: Some(Install::Scripts {
            unix: "curl -fsSL https://code.kimi.com/kimi-code/install.sh | bash",
            windows: "powershell -NoProfile -ExecutionPolicy Bypass -Command \"irm https://code.kimi.com/kimi-code/install.ps1 | iex\"",
        }),
        login: Some("{bin} login"),
        signed_in: None,
    },
    Harness {
        id: "droid",
        name: "Factory Droid",
        bins: &["droid"],
        local: Some("{bin} exec --output-format acp-daemon"),
        registry: Some("factory-droid"),
        setup: "Install Droid (curl -fsSL https://app.factory.ai/cli | sh) and sign in with `droid`.",
        install: Some(Install::Scripts {
            unix: "curl -fsSL https://app.factory.ai/cli | sh",
            windows: "powershell -NoProfile -ExecutionPolicy Bypass -Command \"irm https://app.factory.ai/cli/windows | iex\"",
        }),
        login: Some("{bin}"),
        signed_in: None,
    },
    Harness {
        id: "amp",
        name: "Amp",
        bins: &["amp"],
        local: None,
        registry: Some("amp-acp"),
        setup: "Install Amp (curl -fsSL https://ampcode.com/install.sh | bash) and run `amp login`.",
        install: Some(Install::Scripts {
            unix: "curl -fsSL https://ampcode.com/install.sh | bash",
            windows: "powershell -NoProfile -ExecutionPolicy Bypass -Command \"irm https://ampcode.com/install.ps1 | iex\"",
        }),
        login: Some("{bin} login"),
        signed_in: None,
    },
    Harness {
        id: "kilo",
        name: "Kilo",
        bins: &["kilo"],
        local: Some("{bin} acp"),
        registry: Some("kilo"),
        setup: "Install Kilo CLI (npm install -g @kilocode/cli) and run `kilo auth login`.",
        install: Some(Install::Npm("@kilocode/cli")),
        login: Some("{bin} auth login"),
        signed_in: Some(SignInCheck { args: "profile --json", ok: exit_ok }),
    },
    Harness {
        id: "cline",
        name: "Cline",
        bins: &["cline"],
        local: Some("{bin} --acp"),
        registry: Some("cline"),
        setup: "Install Cline CLI (npm install -g cline) and run `cline auth`.",
        install: Some(Install::Npm("cline")),
        login: Some("{bin} auth"),
        signed_in: None,
    },
    Harness {
        id: "auggie",
        name: "Auggie",
        bins: &["auggie"],
        local: Some("{bin} --acp"),
        registry: Some("auggie"),
        setup: "Install Auggie (npm install -g @augmentcode/auggie) and run `auggie login`.",
        install: Some(Install::Npm("@augmentcode/auggie")),
        login: Some("{bin} login"),
        signed_in: Some(SignInCheck { args: "account status", ok: exit_ok }),
    },
    Harness {
        id: "vibe",
        name: "Mistral Vibe",
        bins: &["vibe-acp", "vibe"],
        local: None,
        registry: Some("mistral-vibe"),
        setup: "Install Mistral Vibe (curl -LsSf https://mistral.ai/vibe/install.sh | bash) and run `vibe --setup`.",
        install: Some(Install::Script("curl -LsSf https://mistral.ai/vibe/install.sh | bash")),
        login: Some("vibe --setup"),
        signed_in: None,
    },
    Harness {
        id: "kiro",
        name: "Kiro CLI",
        bins: &["kiro-cli"],
        local: Some("{bin} acp"),
        registry: None,
        setup: "Install Kiro CLI (curl -fsSL https://cli.kiro.dev/install | bash) and run `kiro-cli login`.",
        install: Some(Install::Scripts {
            unix: "curl -fsSL https://cli.kiro.dev/install | bash",
            windows: "powershell -NoProfile -ExecutionPolicy Bypass -Command \"irm 'https://cli.kiro.dev/install.ps1' | iex\"",
        }),
        login: Some("{bin} login"),
        signed_in: None,
    },
    Harness {
        id: "devin",
        name: "Devin",
        bins: &["devin"],
        local: Some("{bin} acp"),
        registry: Some("devin"),
        setup: "Install the Devin CLI (curl -fsSL https://cli.devin.ai/install.sh | bash) and run `devin auth login`.",
        install: Some(Install::Scripts {
            unix: "curl -fsSL https://cli.devin.ai/install.sh | bash",
            windows: "powershell -NoProfile -ExecutionPolicy Bypass -Command \"irm https://cli.devin.ai/install.ps1 | iex\"",
        }),
        login: Some("{bin} auth login"),
        signed_in: None,
    },
    Harness {
        id: "qoder",
        name: "Qoder CLI",
        bins: &["qodercli"],
        local: Some("{bin} --acp"),
        registry: Some("qoder"),
        setup: "Install Qoder CLI (curl -fsSL https://qoder.com/install | bash) and run `qodercli login`.",
        install: Some(Install::Scripts {
            unix: "curl -fsSL https://qoder.com/install | bash",
            windows: "powershell -NoProfile -ExecutionPolicy Bypass -Command \"irm https://qoder.com/install.ps1 | iex\"",
        }),
        login: Some("{bin} login"),
        signed_in: Some(SignInCheck { args: "status -o json", ok: qoder_signed_in }),
    },
    Harness {
        id: "codebuddy",
        name: "CodeBuddy Code",
        bins: &["codebuddy", "cbc"],
        local: Some("{bin} --acp"),
        registry: Some("codebuddy-code"),
        setup: "Install CodeBuddy Code (npm install -g @tencent-ai/codebuddy-code) and sign in with `codebuddy`.",
        install: Some(Install::Npm("@tencent-ai/codebuddy-code")),
        login: Some("{bin}"),
        signed_in: None,
    },
    Harness {
        id: "minimax",
        name: "MiniMax Code",
        bins: &["mcode"],
        local: Some("{bin} acp"),
        registry: Some("minimax-code"),
        setup: "Install MiniMax Code (npm install -g @minimax-ai/code) and run `mcode login`.",
        install: Some(Install::Npm("@minimax-ai/code")),
        login: Some("{bin} login"),
        signed_in: None,
    },
    Harness {
        id: "junie",
        name: "Junie",
        bins: &["junie"],
        local: Some("{bin} --acp=true"),
        registry: Some("junie"),
        setup: "Install Junie (curl -fsSL https://junie.jetbrains.com/install.sh | bash) and sign in with `junie`.",
        install: Some(Install::Scripts {
            unix: "curl -fsSL https://junie.jetbrains.com/install.sh | bash",
            windows: "powershell -NoProfile -ExecutionPolicy Bypass -Command \"iex (irm 'https://junie.jetbrains.com/install.ps1')\"",
        }),
        login: Some("{bin}"),
        signed_in: None,
    },
    Harness {
        id: "antigravity",
        name: "Google Antigravity",
        bins: &["agy"],
        local: None,
        registry: Some("antigravity-acp"),
        setup: "Install the Antigravity CLI (curl -fsSL https://antigravity.google/cli/install.sh | bash), then sign in from Codync with Log in with Google.",
        install: Some(Install::Scripts {
            unix: "curl -fsSL https://antigravity.google/cli/install.sh | bash",
            windows: "powershell -NoProfile -ExecutionPolicy Bypass -Command \"irm https://antigravity.google/cli/install.ps1 | iex\"",
        }),
        // Signing in with `agy` doesn't sign in its ACP server, which keeps its own sign-in
        // (`~/.gemini/antigravity-acp/settings.json`): only the server's own methods do.
        login: None,
        signed_in: None,
    },
    Harness {
        id: "cortex",
        name: "Cortex Code",
        bins: &["cortex"],
        local: Some("{bin} acp serve"),
        registry: Some("cortex-code"),
        setup: "Install Cortex Code (curl -LsS https://ai.snowflake.com/static/cc-scripts/install.sh | sh) and set up a connection with `cortex`.",
        install: Some(Install::Scripts {
            unix: "curl -LsS https://ai.snowflake.com/static/cc-scripts/install.sh | sh",
            windows: "powershell -NoProfile -ExecutionPolicy Bypass -Command \"irm https://ai.snowflake.com/static/cc-scripts/install.ps1 | iex\"",
        }),
        login: Some("{bin}"),
        signed_in: None,
    },
    Harness {
        id: "poolside",
        name: "Poolside",
        bins: &["pool"],
        local: Some("{bin} acp"),
        registry: Some("poolside"),
        setup: "Install Poolside (curl -fsSL https://downloads.poolside.ai/pool/install.sh | sh) and run `pool login`.",
        install: Some(Install::Scripts {
            unix: "curl -fsSL https://downloads.poolside.ai/pool/install.sh | sh",
            windows: "powershell -NoProfile -ExecutionPolicy Bypass -Command \"irm https://downloads.poolside.ai/pool/install.ps1 | iex\"",
        }),
        login: Some("{bin} login"),
        signed_in: None,
    },
];

pub fn harness(id: &str) -> Option<&'static Harness> {
    HARNESSES.iter().find(|h| h.id == id)
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
            let native = path.is_some() && h.local.is_some();
            let runnable = native || reg.is_some_and(|a| registry::launch_kind(a).is_some());
            let node = if native { None } else { reg.filter(|a| registry::needs_node(a)).and_then(|_| node::state()) };
            json!({
                "id": h.id,
                "name": h.name,
                "installed": path.is_some(),
                "available": path.is_some() && runnable,
                "path": path.map(|p| p.to_string_lossy().into_owned()),
                "description": reg.and_then(|a| a["description"].as_str()).unwrap_or_default(),
                "installHint": h.setup,
                "signedIn": signed_in(h.id),
                "canInstall": h.install.and_then(Install::command).is_some(),
                "node": node.map(node::State::name),
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
            "signedIn": signed_in(id),
            "node": node::state().filter(|_| registry::needs_node(a)).map(node::State::name),
            "command": "",
            "registry": id,
            "curated": false,
        }));
    }
    out.sort_by_key(|v| (!v["installed"].as_bool().unwrap_or(false), !v["curated"].as_bool().unwrap_or(false)));
    out
}

/// `program` is some other harness's CLI (not `backend`'s own).
pub fn belongs_to_other(backend: &str, program: &str) -> bool {
    let name = Path::new(program).file_name().and_then(|n| n.to_str()).unwrap_or(program);
    let own = |h: &Harness| h.id == backend || h.registry == Some(backend);
    HARNESSES.iter().any(|h| !own(h) && h.bins.contains(&name))
        && !HARNESSES.iter().any(|h| own(h) && h.bins.contains(&name))
}

pub fn is_known(id: &str) -> bool {
    harness(id).is_some() || registry::agent(id).is_some()
}

/// Ways to start this backend, best first: the installed CLI's own ACP mode, then the
/// registry build, for a CLI that isn't installed, has no ACP mode of its own, or is too old for it.
pub fn launch_candidates(id: &str) -> anyhow::Result<Vec<registry::Candidate>> {
    let h = harness(id);
    let build = h.map_or(Some(id), |h| h.registry).and_then(registry::agent);
    let out: Vec<_> = local_candidate(id)
        .map(registry::Candidate::Ready)
        .into_iter()
        .chain(build.map(registry::Candidate::Build))
        .collect();
    if out.is_empty() {
        let hint = h.map_or("Check that it's installed on this computer.", |h| h.setup);
        let name = h.map_or(id, |h| h.name);
        return Err(acp::Failure::error(
            acp::FailureKind::NotSetUp,
            format!("{name} isn't set up on this computer. {hint}"),
        ));
    }
    Ok(out)
}

/// The registry build that *is* the harness's CLI (same npm package), so it can
/// sign in without the CLI installed. Adapters (claude-acp…) don't count.
fn registry_cli(h: &Harness) -> Option<Value> {
    let Some(Install::Npm(pkg)) = h.install else { return None };
    let agent = registry::agent(h.registry?)?;
    let npx = agent["distribution"]["npx"]["package"].as_str()?;
    (npx == pkg || npx.strip_prefix(pkg).is_some_and(|v| v.starts_with('@'))).then_some(agent)
}

/// Codync's own sign-in command for `id` can run now.
pub fn login_available(id: &str) -> bool {
    harness(id).is_some_and(|h| {
        h.login.is_some_and(|login| {
            h.bins.iter().any(|b| on_path(b)) || !login.contains("{bin}") || registry_cli(h).is_some()
        })
    })
}

/// Codync's own sign-in command for `id`, with `{bin}` resolved.
pub async fn login_command(id: &str) -> anyhow::Result<String> {
    let (h, login) = harness(id)
        .and_then(|h| h.login.map(|login| (h, login)))
        .ok_or_else(|| anyhow::anyhow!("no sign-in command for {id}"))?;
    let bin = match h.bins.iter().find_map(|b| which(b)) {
        Some(p) => shell::quote(&p.to_string_lossy()),
        None => match registry_cli(h) {
            Some(agent) => registry::command(&agent, |_| {}).await?.program,
            None if !login.contains("{bin}") => String::new(),
            None => anyhow::bail!("Install {} first.", h.name),
        },
    };
    Ok(login.replace("{bin}", &bin))
}

/// The installed CLI's own ACP mode, if it has one and it's installed.
fn local_candidate(id: &str) -> Option<registry::Cmd> {
    let h = harness(id)?;
    let template = h.local?;
    let path = h.bins.iter().find_map(|b| which(b))?;
    let args = template.strip_prefix("{bin}").unwrap_or(template).trim().to_owned();
    Some(registry::Cmd { program: shell::quote(&path.to_string_lossy()), args })
}

#[cfg(test)]
mod tests;
