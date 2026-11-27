//! `codync-host network setup`: puts this host's `codync` MCP server, and the hooks that tell a
//! session of new letters (`hooks.rs`), into the coding agents installed here. The server goes in
//! through each agent's own `mcp add` where it has one. A `codync` entry that isn't this host's is
//! left alone, and `--remove` takes out only ours.

use super::HookAgent;
use crate::analytics::reported::{App, Reported, SetupFailure, SetupOutcome, Track};
use anyhow::{Context as _, Result, bail};
use clap::ValueEnum as _;
use codync_agent::{mcp_names, shell};
use codync_network::AgentKind;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;

mod hooks;

/// The name each agent knows our server by.
const NAME: &str = "codync";

#[derive(Clone, Copy, PartialEq, Eq, Debug, clap::ValueEnum)]
pub enum SetupAgent {
    Claude,
    Codex,
    Copilot,
    Grok,
    Pi,
    Antigravity,
    Cursor,
    Opencode,
}

impl SetupAgent {
    fn kind(self) -> AgentKind {
        match self {
            Self::Claude => AgentKind::ClaudeCode,
            Self::Codex => AgentKind::Codex,
            Self::Copilot => AgentKind::Copilot,
            Self::Grok => AgentKind::Grok,
            Self::Pi => AgentKind::Pi,
            Self::Antigravity => AgentKind::Antigravity,
            Self::Cursor => AgentKind::Cursor,
            Self::Opencode => AgentKind::Opencode,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Copilot => "Copilot CLI",
            Self::Grok => "Grok",
            Self::Pi => "pi",
            Self::Antigravity => "Antigravity",
            Self::Cursor => "Cursor",
            Self::Opencode => "opencode",
        }
    }

    /// Its CLI, which adds and removes MCP servers (Cursor's has no `add`; opencode's asks).
    fn cli(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Copilot => "copilot",
            Self::Grok => "grok",
            Self::Pi => "pi",
            Self::Antigravity => "agy",
            Self::Cursor => "cursor-agent",
            Self::Opencode => "opencode",
        }
    }

    /// `mcp add`/`mcp remove`'s scope flags: the user's config, for every project.
    fn scope(self) -> &'static [&'static str] {
        match self {
            Self::Claude | Self::Grok => &["-s", "user"],
            _ => &[],
        }
    }

    /// Its own config folder: where the variable that moves it points, else its default under `home`.
    fn dir(self, home: &Path) -> PathBuf {
        let (var, default) = match self {
            Self::Claude => (Some("CLAUDE_CONFIG_DIR"), ""),
            Self::Codex => (Some(mcp_names::Agent::Codex.home_var()), ".codex"),
            Self::Copilot => (Some("COPILOT_HOME"), ".copilot"),
            Self::Grok => (Some("GROK_HOME"), ".grok"),
            Self::Pi => (Some(mcp_names::Agent::Pi.home_var()), ".pi/agent"),
            Self::Antigravity => (None, ".gemini/config"),
            Self::Cursor => (None, ".cursor"),
            Self::Opencode => (Some("XDG_CONFIG_HOME"), ".config"),
        };
        var.and_then(std::env::var_os).filter(|d| !d.is_empty()).map_or_else(|| home.join(default), PathBuf::from)
    }

    /// The file its user-wide MCP servers live in, read to tell ours from another `codync`.
    fn mcp_file(self, home: &Path) -> PathBuf {
        let dir = self.dir(home);
        match self {
            Self::Claude => dir.join(".claude.json"),
            Self::Codex | Self::Grok => dir.join("config.toml"),
            Self::Copilot => dir.join("mcp-config.json"),
            Self::Pi | Self::Cursor => dir.join("mcp.json"),
            Self::Antigravity => dir.join("mcp_config.json"),
            // Its JSONC is edited only while it parses as plain JSON (no comments).
            Self::Opencode => {
                let dir = dir.join("opencode");
                let jsonc = dir.join("opencode.jsonc");
                if jsonc.exists() { jsonc } else { dir.join("opencode.json") }
            }
        }
    }

    /// The table of that file its MCP servers are in.
    fn section(self) -> &'static str {
        match self {
            Self::Codex | Self::Grok => "mcp_servers",
            Self::Opencode => "mcp",
            _ => "mcpServers",
        }
    }

    /// Its hooks: only the agents whose hooks were checked to name their sessions the way their MCP
    /// servers do (`network hook`).
    fn hook(self) -> Option<HookAgent> {
        match self {
            Self::Codex => Some(HookAgent::Codex),
            Self::Copilot => Some(HookAgent::Copilot),
            Self::Grok => Some(HookAgent::Grok),
            _ => None,
        }
    }

    /// How soon a session hears of a letter once set up.
    fn tier(self) -> &'static str {
        if cfg!(unix) && self == Self::Claude {
            "sessions are told as soon as a letter comes: an idle one starts a turn (at most 12 an hour)"
        } else if cfg!(unix) && self.hook().is_some() {
            "sessions are told of new letters at their next step"
        } else {
            "sessions read their letters when they check (check_inbox)"
        }
    }

    fn installed(self, home: &Path) -> bool {
        match self {
            Self::Cursor => self.dir(home).is_dir(),
            _ => codync_agent::backends::which(self.cli()).is_some(),
        }
    }
}

/// What this host runs as: its own binary, with the port when it isn't the default.
pub struct Want {
    exe: String,
    port: Option<u16>,
}

impl Want {
    pub fn new(port: u16) -> Result<Self> {
        let exe = linked(std::env::current_exe().context("finding codync-host")?);
        let port = (port != crate::service::DEFAULT_PORT).then_some(port);
        Ok(Self { exe: exe.to_string_lossy().into_owned(), port })
    }

    fn port_args(&self) -> Vec<String> {
        self.port.map(|p| vec!["--port".into(), p.to_string()]).unwrap_or_default()
    }

    /// The MCP server's arguments after the binary.
    fn mcp_args(&self) -> Vec<String> {
        [vec!["mcp".into(), "network".into()], self.port_args()].concat()
    }

    /// The command line of `agent`'s hook.
    fn hook_command(&self, agent: HookAgent) -> String {
        let value = agent.to_possible_value().expect("no HookAgent is skipped");
        let mut line = format!("{} network hook {}", shell::quote(&self.exe), value.get_name());
        for a in self.port_args() {
            line.push(' ');
            line.push_str(&a);
        }
        line
    }
}

/// Homebrew's link to `exe` when `exe` is in a versioned keg (Linux's `current_exe` resolves
/// links), which the next `brew upgrade` deletes.
fn linked(exe: PathBuf) -> PathBuf {
    let link =
        exe.to_string_lossy().split_once("/Cellar/").map(|(prefix, _)| Path::new(prefix).join("bin/codync-host"));
    link.filter(|l| l.exists()).unwrap_or(exe)
}

/// An agent's `codync` MCP server, as its config has it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    Absent,
    /// This host's, as it should be.
    Current,
    /// A codync-host network server, at another path or with other arguments (an older install).
    Stale,
    /// Someone else's server under our name.
    Foreign,
}

fn entry(server: Option<&Value>, want: &Want) -> Entry {
    let Some(server) = server else { return Entry::Absent };
    // Turned off in the agent's config (Codex, opencode `enabled`, Antigravity `disabled`): added again, on.
    let off = server["enabled"] == false || server["disabled"] == true;
    let server = normalized(server);
    let command = server["command"].as_str().unwrap_or_default();
    let args: Vec<&str> = server["args"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let ours =
        Path::new(command).file_stem().is_some_and(|s| s == "codync-host") && args.starts_with(&["mcp", "network"]);
    if !ours {
        Entry::Foreign
    } else if command == want.exe && args == want.mcp_args() && !off {
        Entry::Current
    } else {
        Entry::Stale
    }
}

/// A server as `{command, args}`: opencode keeps the binary and its arguments in one array.
fn normalized(server: &Value) -> Value {
    match server["command"].as_array().map(Vec::as_slice) {
        Some([command, args @ ..]) => json!({"command": command, "args": args}),
        _ => server.clone(),
    }
}

/// What setup did for an agent, or found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Done {
    Added,
    /// As it should be (with `--remove`: nothing of ours).
    Current,
    Foreign,
    Removed,
}

impl Done {
    fn outcome(self) -> SetupOutcome {
        match self {
            Self::Added => SetupOutcome::Added,
            Self::Current => SetupOutcome::Current,
            Self::Foreign => SetupOutcome::Foreign,
            Self::Removed => SetupOutcome::Removed,
        }
    }
}

/// A step's error message, with the kind of failure it is counted as.
#[derive(Debug)]
struct Failed(SetupFailure, String);

impl std::fmt::Display for Failed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.1)
    }
}

fn unreadable(file: &Path) -> Failed {
    Failed(SetupFailure::UnreadableConfig, format!("reading {}", file.display()))
}

/// The `codync` server under `section` in `file` (JSON, or TOML); none when there's no file. A file
/// that doesn't parse is an error: nothing is written next to what can't be read.
fn read_server(file: &Path, section: &str) -> Result<Option<Value>> {
    let Some(text) = read(file)? else { return Ok(None) };
    if file.extension().is_some_and(|e| e == "toml") {
        let doc: toml_edit::DocumentMut = text.parse().with_context(|| unreadable(file))?;
        let Some(server) = doc.get(section).and_then(|s| s.get(NAME)) else { return Ok(None) };
        let args: Vec<&str> =
            server.get("args").and_then(|a| a.as_array()).into_iter().flatten().filter_map(|a| a.as_str()).collect();
        let enabled = server.get("enabled").and_then(toml_edit::Item::as_bool);
        return Ok(Some(
            json!({"command": server.get("command").and_then(|c| c.as_str()), "args": args, "enabled": enabled}),
        ));
    }
    Ok(parse(file, &text)?[section].get(NAME).cloned())
}

fn read(file: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(file) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| unreadable(file)),
    }
}

fn parse(file: &Path, text: &str) -> Result<Value> {
    serde_json::from_str(text.trim_start_matches('\u{feff}')).with_context(|| {
        Failed(
            SetupFailure::UnreadableConfig,
            format!("reading {} (plain JSON only: add it by hand if it has comments)", file.display()),
        )
    })
}

fn json_file(file: &Path) -> Result<Value> {
    read(file)?.map_or(Ok(json!({})), |t| parse(file, &t))
}

/// Writes `file` whole, through a temporary file beside it, so an agent never reads half of it. A
/// link is followed (a dotfiles repo's file stays the one in use), and the file keeps its mode.
fn write(file: &Path, value: &Value) -> Result<()> {
    let wrote = || -> std::io::Result<()> {
        let target = linked_target(file)?;
        let mode = target.exists().then(|| std::fs::metadata(&target).map(|m| m.permissions())).transpose()?;
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // One of its own, made new: two setups at once never write into the same one.
        let tmp = target.with_extension(format!("codync-{}.tmp", uuid::Uuid::new_v4().simple()));
        // Private from its first byte: the config may hold other servers' tokens.
        let mut open = std::fs::OpenOptions::new();
        open.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut open, 0o600);
        let text = serde_json::to_string_pretty(value).map_err(std::io::Error::other)? + "\n";
        std::io::Write::write_all(&mut open.open(&tmp)?, text.as_bytes())?;
        if let Some(mode) = mode {
            std::fs::set_permissions(&tmp, mode)?;
        }
        std::fs::rename(&tmp, &target)
    };
    wrote().with_context(|| Failed(SetupFailure::Write, format!("writing {}", file.display())))
}

/// The file a write to `file` should change: a link's target (also one not created yet), followed
/// to its end.
fn linked_target(file: &Path) -> std::io::Result<PathBuf> {
    let mut path = file.to_owned();
    for _ in 0..32 {
        match std::fs::symlink_metadata(&path) {
            Ok(m) if m.file_type().is_symlink() => {
                let to = std::fs::read_link(&path)?;
                path = path.parent().map_or(to.clone(), |dir| dir.join(&to));
            }
            Ok(_) => return std::fs::canonicalize(&path),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(path),
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::other("too many links"))
}

/// Runs `agent`'s CLI (in `dir`, when given); its own message when it refuses.
fn cli(agent: SetupAgent, args: &[&str], dir: Option<&Path>) -> Result<()> {
    let mut command = Command::new(shell::program(agent.cli()));
    command.args(args);
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    let out = command.output().with_context(|| Failed(SetupFailure::CliMissing, format!("running {}", agent.cli())))?;
    if !out.status.success() {
        let said = String::from_utf8_lossy(&out.stderr).trim().to_owned() + String::from_utf8_lossy(&out.stdout).trim();
        bail!(Failed(SetupFailure::CliRefused, format!("{} mcp: {said}", agent.cli())));
    }
    Ok(())
}

/// Adds our server to `agent`, replacing an older one of ours; what it found there.
fn add_server(agent: SetupAgent, home: &Path, want: &Want) -> Result<Entry> {
    let file = agent.mcp_file(home);
    let found = entry(read_server(&file, agent.section())?.as_ref(), want);
    match found {
        Entry::Current | Entry::Foreign => return Ok(found),
        Entry::Stale => remove_server(agent, home)?,
        Entry::Absent => {}
    }
    match agent {
        // No `mcp add` (Cursor) or one that asks (opencode): its file, our key only.
        SetupAgent::Cursor => {
            set_key(&file, agent.section(), Some(json!({"command": want.exe, "args": want.mcp_args()})))?;
        }
        SetupAgent::Opencode => {
            let command = [vec![want.exe.clone()], want.mcp_args()].concat();
            set_key(&file, agent.section(), Some(json!({"type": "local", "command": command})))?;
        }
        _ => {
            let mut args = vec!["mcp", "add"];
            args.extend(agent.scope());
            args.extend([NAME, "--", &want.exe]);
            let rest = want.mcp_args();
            args.extend(rest.iter().map(String::as_str));
            cli(agent, &args, None)?;
        }
    }
    Ok(found)
}

/// Takes our server out of `agent`.
fn remove_server(agent: SetupAgent, home: &Path) -> Result<()> {
    match agent {
        SetupAgent::Cursor | SetupAgent::Opencode => set_key(&agent.mcp_file(home), agent.section(), None),
        _ => {
            let mut args = vec!["mcp", "remove"];
            args.extend(agent.scope());
            args.push(NAME);
            cli(agent, &args, None)
        }
    }
}

/// Sets (or, with `None`, deletes) `codync` under `section` in the JSON `file`, keeping the rest.
fn set_key(file: &Path, section: &str, value: Option<Value>) -> Result<()> {
    let mut config = json_file(file)?;
    let shape =
        || Failed(SetupFailure::UnreadableConfig, format!("{section} in {} isn't a JSON object", file.display()));
    let Some(root) = config.as_object_mut() else { bail!(shape()) };
    let servers = root.entry(section).or_insert_with(|| json!({}));
    let Some(servers) = servers.as_object_mut() else { bail!(shape()) };
    match value {
        Some(v) => servers.insert(NAME.into(), v),
        None => servers.remove(NAME),
    };
    write(file, &config)
}

/// The Claude Code projects (folders) whose own `codync` (local scope, which wins over the user's)
/// is an older one of ours, or with `all`, any of ours.
fn claude_projects(config: &Value, want: &Want, all: bool) -> Vec<PathBuf> {
    config["projects"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, project)| match entry(project["mcpServers"].get(NAME), want) {
            Entry::Stale => true,
            Entry::Current => all,
            Entry::Absent | Entry::Foreign => false,
        })
        .map(|(dir, _)| PathBuf::from(dir))
        .collect()
}

/// Takes those servers out of the projects still on disk; whether there were any.
fn clean_claude_projects(home: &Path, want: &Want, all: bool) -> Result<bool> {
    let file = SetupAgent::Claude.mcp_file(home);
    let Some(text) = read(&file)? else { return Ok(false) };
    let dirs: Vec<PathBuf> =
        claude_projects(&parse(&file, &text)?, want, all).into_iter().filter(|d| d.is_dir()).collect();
    for dir in &dirs {
        cli(SetupAgent::Claude, &["mcp", "remove", "-s", "local", NAME], Some(dir))
            .with_context(|| format!("in the project {}", dir.display()))?;
    }
    Ok(!dirs.is_empty())
}

fn add_one(agent: SetupAgent, home: &Path, want: &Want, hooks: Option<&(HookAgent, PathBuf)>) -> Result<Done> {
    // Someone else's user-wide `codync`: nothing of ours is touched, the projects' entries included.
    if entry(read_server(&agent.mcp_file(home), agent.section())?.as_ref(), want) == Entry::Foreign {
        return Ok(Done::Foreign);
    }
    let cleaned = agent == SetupAgent::Claude && clean_claude_projects(home, want, false)?;
    let server = add_server(agent, home, want)?;
    let hooked = match hooks {
        Some((hook, file)) => hooks::add(file, *hook, &want.hook_command(*hook))?,
        None => false,
    };
    Ok(if cleaned || hooked || server != Entry::Current { Done::Added } else { Done::Current })
}

fn remove_one(agent: SetupAgent, home: &Path, want: &Want, hooks: Option<&(HookAgent, PathBuf)>) -> Result<Done> {
    let cleaned = agent == SetupAgent::Claude && clean_claude_projects(home, want, true)?;
    let found = entry(read_server(&agent.mcp_file(home), agent.section())?.as_ref(), want);
    let ours = matches!(found, Entry::Current | Entry::Stale);
    if ours {
        remove_server(agent, home)?;
    }
    let hooked = match hooks {
        Some((hook, file)) => hooks::remove(file, *hook)?,
        None => false,
    };
    Ok(if cleaned || ours || hooked { Done::Removed } else { Done::Current })
}

/// Sets up (or removes) one agent; the line to print, the outcome to count and why it failed.
fn one(agent: SetupAgent, home: &Path, want: &Want, remove: bool) -> (String, SetupOutcome, Option<SetupFailure>) {
    let hooks = agent.hook().filter(|_| cfg!(unix)).map(|h| (h, hooks::file(h, &agent.dir(home))));
    let result =
        if remove { remove_one(agent, home, want, hooks.as_ref()) } else { add_one(agent, home, want, hooks.as_ref()) };
    match result {
        Ok(done) => (line(agent, done, home, remove), done.outcome(), None),
        Err(e) => {
            let what = if remove { "remove" } else { "set up" };
            let failure = e.downcast_ref::<Failed>().map_or(SetupFailure::Other, |f| f.0);
            (format!("{}: couldn't {what}: {e:#}", agent.label()), SetupOutcome::Failed, Some(failure))
        }
    }
}

fn line(agent: SetupAgent, done: Done, home: &Path, remove: bool) -> String {
    let name = agent.label();
    match done {
        Done::Added => {
            let approve =
                if agent == SetupAgent::Codex && cfg!(unix) { " Approve its hooks once in /hooks." } else { "" };
            format!("{name}: set up; {}.{approve}", agent.tier())
        }
        Done::Current if remove => format!("{name}: nothing of ours to remove."),
        Done::Current => format!("{name}: already set up; {}.", agent.tier()),
        Done::Foreign => format!(
            "{name}: left alone: its \"{NAME}\" MCP server in {} isn't this computer's network server. Remove it, or rename it, and run setup again.",
            agent.mcp_file(home).display()
        ),
        Done::Removed => format!("{name}: removed."),
    }
}

/// Sets up `agents` (none: every one installed here), prints a line for each and counts it.
pub async fn run(port: u16, agents: Vec<SetupAgent>, remove: bool) -> Result<()> {
    let want = Want::new(port)?;
    let home = crate::service::home();
    let done = tokio::task::spawn_blocking(move || {
        let picked = if agents.is_empty() {
            SetupAgent::value_variants().iter().copied().filter(|a| a.installed(&home)).collect()
        } else {
            agents
        };
        picked.into_iter().map(|a| (a, one(a, &home, &want, remove))).collect::<Vec<_>>()
    })
    .await?;
    if done.is_empty() {
        println!(
            "No coding agent this works with is installed here (Claude Code, Codex, Copilot CLI, Grok, pi, Antigravity, Cursor, opencode)."
        );
    }
    // The agents start the server without this shell's environment: it finds the default data folder.
    if !remove && std::env::var_os("CODYNC_HOME").is_some() {
        println!(
            "CODYNC_HOME is set here, but agents start the codync server without it: they reach the host in the default data folder."
        );
    }
    if !remove && cfg!(windows) {
        println!(
            "On Windows, sessions read their letters when they check (check_inbox): hooks and push aren't set up here yet."
        );
    }
    let failed = done.iter().filter(|(_, (_, outcome, _))| *outcome == SetupOutcome::Failed).count();
    for (agent, (text, outcome, error)) in done {
        println!("{text}");
        let body = Track::body(App::Cli, Reported::NetworkSetup { agent: agent.kind(), outcome, error });
        // Counted only when the host runs; setup itself never needs it.
        if let Err(e) = crate::local_api::local_call_for(port, "track", body, std::time::Duration::from_secs(2)).await {
            tracing::debug!(error = format!("{e:#}"), "network setup wasn't counted");
        }
    }
    if failed > 0 {
        bail!("{failed} agent(s) couldn't be {}", if remove { "cleaned up" } else { "set up" });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
