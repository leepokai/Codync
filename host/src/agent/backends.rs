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

mod catalog;
mod login_env;
mod search_path;
mod sign_in;
mod workbuddy;
pub use catalog::HARNESSES;
pub use login_env::vars as login_env;
pub use search_path::{hydrate_path, on_path, which};
pub use sign_in::{
    SignInCheck, apply_prompt_refusal, clear_prompt_refused, cli_signed_in, note_prompt_refused, refresh_sign_in,
    set_signed_in, signed_in,
};
pub use workbuddy::{apply_process_env, move_saved_session_aside, session_proves_sign_in};

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

pub fn harness(id: &str) -> Option<&'static Harness> {
    HARNESSES.iter().find(|h| h.id == id)
}

// MARK: listing + launching

/// Every harness the phone can pick, installed ones first.
pub fn list() -> Vec<Value> {
    let registry = registry::agents();
    let mut out: Vec<Value> = HARNESSES
        .iter()
        .filter(|h| workbuddy::offered(h.id))
        .map(|h| {
            let path = installed_cli(h);
            let reg = h.registry.and_then(|id| registry.iter().find(|a| a["id"] == id));
            let native = path.is_some() && h.local.is_some();
            let runnable = native || reg.is_some_and(|a| registry::launch_kind(a).is_some());
            let needs_node = if native { workbuddy::runs_on_node(h.id) } else { reg.is_some_and(registry::needs_node) };
            let node = if needs_node { node::state() } else { None };
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
    // WorkBuddy's CLI shares CodeBuddy Code's file name. The app path decides which one it is.
    if workbuddy::is_cli(program) {
        return backend != "workbuddy";
    }
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
        h.login.is_some_and(|login| installed_cli(h).is_some() || !login.contains("{bin}") || registry_cli(h).is_some())
    })
}

/// Codync's own sign-in command for `id`, with `{bin}` resolved.
pub async fn login_command(id: &str) -> anyhow::Result<String> {
    let (h, login) = harness(id)
        .and_then(|h| h.login.map(|login| (h, login)))
        .ok_or_else(|| anyhow::anyhow!("no sign-in command for {id}"))?;
    let bin = match installed_cli(h) {
        Some(p) => shell::quote(&p.to_string_lossy()),
        None => match registry_cli(h) {
            Some(agent) => registry::command(&agent, |_| {}).await?.program,
            None if !login.contains("{bin}") => String::new(),
            None => anyhow::bail!("Install {} first.", h.name),
        },
    };
    Ok(login.replace("{bin}", &bin))
}

/// Where this harness's CLI is installed: on `PATH`, or inside the `WorkBuddy` app.
fn installed_cli(h: &Harness) -> Option<std::path::PathBuf> {
    h.bins.iter().find_map(|b| which(b)).or_else(|| (h.id == "workbuddy").then(workbuddy::find).flatten())
}

/// The installed CLI's own ACP mode, if it has one and it's installed.
fn local_candidate(id: &str) -> Option<registry::Cmd> {
    let h = harness(id)?;
    let template = h.local?;
    let path = installed_cli(h)?;
    let args = template.strip_prefix("{bin}").unwrap_or(template).trim().to_owned();
    Some(registry::Cmd { program: shell::quote(&path.to_string_lossy()), args })
}

#[cfg(test)]
mod tests;
