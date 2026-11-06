//! Servers a bot left running through its own shell. They can't leave the agent's tree (a job on Windows holds them,
//! and on Unix a started process has no new parent to go to), so they only show, marked as stopping with the bot.

use super::ports::{self, Listening, Port};
use super::{Status, View, platform, report};
use crate::LockExt;
use crate::agent::acp::Acp;
use crate::agent::backends;
use crate::agent::process::{self, JobProcess, program_name};
use crate::analytics::Event;
use crate::hub::Hub;
use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

const ID_PREFIX: &str = "detected:";
/// Above this is where systems pick a port for whoever asks for any: an agent's own callback or inspector, not a
/// server someone would open.
// ponytail: guesses by range; ask agents what they listen on if a real server sits above it.
const HIGHEST_PORT: u16 = 32767;
/// What launches an agent and runs servers just as well, so its name says nothing about the agent.
const RUNTIMES: [&str; 20] = [
    "node",
    "npx",
    "npm",
    "pnpm",
    "yarn",
    "bun",
    "bunx",
    "deno",
    "python",
    "python3",
    "uv",
    "uvx",
    "ruby",
    "java",
    "sh",
    "bash",
    "zsh",
    "cmd",
    "powershell",
    "pwsh",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detected {
    /// Everything holding its sockets, lowest first (a pre-fork server has several); the first names it.
    pub pids: Vec<u32>,
    pub name: String,
    pub sockets: Vec<SocketAddr>,
    pub ports: Vec<Port>,
}

impl Detected {
    pub(super) fn view(&self) -> View {
        View {
            id: format!("{ID_PREFIX}{}", self.pids[0]),
            name: self.name.clone(),
            command: self.name.clone(),
            folder: None,
            status: Status::Running,
            ports: self.ports.clone(),
            started_at: None,
            exit_code: None,
            managed: false,
        }
    }
}

/// An agent to look under.
pub struct RunningAgent {
    pub bot: String,
    pub acp: Arc<Acp>,
    pub programs: Vec<String>,
}

/// The names an agent's own processes go by: its harness's CLIs and the program its command line runs. They can listen
/// below [`HIGHEST_PORT`] (`opencode acp` does) and stay running under a wrapper script or `cmd`.
pub fn agent_programs(backend: &str, command: &str) -> Vec<String> {
    let mut programs: Vec<String> = backends::bins(backend).iter().map(|bin| (*bin).to_owned()).collect();
    let name = plain_name(first_word(command));
    let says_nothing = name.is_empty() || RUNTIMES.contains(&name) || name.starts_with("python");
    if !says_nothing && !programs.iter().any(|program| program == name) {
        programs.push(name.to_owned());
    }
    programs
}

/// The command line's program, past any leading `KEY=value`, without its quotes.
fn first_word(command: &str) -> &str {
    let mut rest = command.trim_start();
    loop {
        if let Some(quote) = rest.chars().next().filter(|c| matches!(c, '\'' | '"')) {
            return rest[1..].split(quote).next().unwrap_or_default();
        }
        let (word, after) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        if after.is_empty() || !word.contains('=') {
            return word;
        }
        rest = after.trim_start();
    }
}

/// A program's name without its folder or Windows' extension.
fn plain_name(program: &str) -> &str {
    let name = program_name(program);
    name.strip_suffix(".exe").or_else(|| name.strip_suffix(".EXE")).unwrap_or(name)
}

/// Ends a detected server of `bot` named by `id`. False when `id` isn't one, so it's a server of the host's own.
pub async fn stop(hub: &Hub, bot: &str, id: &str) -> Result<bool> {
    let Some(first) = id.strip_prefix(ID_PREFIX) else { return Ok(false) };
    let first: u32 = first.parse()?;
    // The last scan can be seconds old and a pid gets reused, so only what's in the agent's tree right now. Read before
    // its row goes: a failed read or a request that goes away leaves it listed. With no agent, its tree ended with it.
    let agent = hub.dev_servers.agents_running().into_iter().find(|agent| agent.bot == bot);
    let tree = match agent {
        Some(agent) => agent_tree(&agent).await.context("couldn't check the agent's processes")?,
        None => Vec::new(),
    };
    let server = hub.dev_servers.detected.locked().get_mut(bot).and_then(|servers| {
        let at = servers.iter().position(|server| server.pids[0] == first)?;
        Some(servers.remove(at))
    });
    let Some(server) = server else { bail!("unknown dev server") };
    for pid in still_in_tree(&server.pids, &tree) {
        platform::end_process(pid);
    }
    hub.emit_bot(bot);
    Ok(true)
}

/// Those of `pids` the agent's tree still holds.
pub(super) fn still_in_tree(pids: &[u32], tree: &[JobProcess]) -> Vec<u32> {
    pids.iter().copied().filter(|pid| tree.iter().any(|process| process.pid == *pid)).collect()
}

/// Finds what each bot's agent left listening, and tells clients when that changed. `moved`: this computer's addresses
/// changed, so the ones worked out before no longer hold.
pub async fn scan(hub: &Hub, agents: &[RunningAgent], listening: &[Listening], moved: bool) {
    let own_program = std::env::current_exe().ok().and_then(|exe| Some(exe.file_name()?.to_str()?.to_owned()));
    let Some(own_program) = own_program else { return };
    let Some(trees) = agent_trees(agents).await else { return };
    let mut found: HashMap<String, Vec<Detected>> = HashMap::new();
    for (agent, tree) in agents.iter().zip(&trees) {
        let known = hub.dev_servers.detected.locked().get(&agent.bot).cloned().unwrap_or_default();
        let mut servers = servers_in(tree, &own_program, &agent.programs, listening);
        for server in &mut servers {
            let same = known.iter().find(|before| before.sockets == server.sockets).filter(|_| !moved);
            server.ports = if let Some(before) = same {
                before.ports.clone()
            } else {
                let sockets = server.sockets.clone();
                tokio::task::spawn_blocking(move || ports::urls(&sockets)).await.unwrap_or_default()
            };
        }
        if !servers.is_empty() {
            found.insert(agent.bot.clone(), servers);
        }
    }
    let (changed, new) = {
        let mut detected = hub.dev_servers.detected.locked();
        let changes = changes(&detected, &found);
        *detected = found;
        changes
    };
    for _ in 0..new {
        report(hub, Event::DevServerDetected, "kind", "shell");
    }
    for bot in changed {
        hub.emit_bot(&bot);
    }
}

/// The bots whose list changed, and how many servers are new.
pub(super) fn changes(
    before: &HashMap<String, Vec<Detected>>,
    found: &HashMap<String, Vec<Detected>>,
) -> (Vec<String>, usize) {
    let mut bots: Vec<String> =
        found.iter().filter(|(bot, servers)| before.get(*bot) != Some(*servers)).map(|(bot, _)| bot.clone()).collect();
    bots.extend(before.keys().filter(|bot| !found.contains_key(*bot)).cloned());
    let is_new = |bot: &str, server: &Detected| {
        !before.get(bot).is_some_and(|known| known.iter().any(|known| known.pids[0] == server.pids[0]))
    };
    let new = found.iter().map(|(bot, servers)| servers.iter().filter(|server| is_new(bot, server)).count()).sum();
    (bots, new)
}

/// What in `tree` listens, one entry per server: processes that share a socket are one server, and neither Codync's
/// helpers nor the agent's own processes are one. Its addresses are worked out later.
pub(super) fn servers_in(
    tree: &[JobProcess],
    own_program: &str,
    programs: &[String],
    listening: &[Listening],
) -> Vec<Detected> {
    let is_ours = process::under_program(tree, own_program);
    let is_agent = |p: &JobProcess| programs.iter().any(|own| plain_name(&p.name).eq_ignore_ascii_case(own));
    let mut outside: Vec<u32> = tree.iter().filter(|p| !is_ours(p) && !is_agent(p)).map(|p| p.pid).collect();
    outside.sort_unstable();
    let mut servers: Vec<Detected> = Vec::new();
    for pid in outside {
        let sockets: Vec<SocketAddr> =
            ports::sockets_of(listening, &[pid]).into_iter().filter(|socket| socket.port() <= HIGHEST_PORT).collect();
        if sockets.is_empty() {
            continue;
        }
        let sharing = servers.iter_mut().find(|server| server.sockets.iter().any(|socket| sockets.contains(socket)));
        if let Some(server) = sharing {
            server.pids.push(pid);
            server.sockets.extend(sockets);
            server.sockets.sort();
            server.sockets.dedup();
        } else {
            let name = listening.iter().find(|l| l.pid == pid).map_or_else(String::new, |l| l.name.clone());
            servers.push(Detected { pids: vec![pid], name, sockets, ports: Vec::new() });
        }
    }
    servers
}

/// What each of `agents` started, without the agent itself, listed once per scan: `None` when that failed, so nothing
/// is taken to have ended.
#[cfg(unix)]
async fn agent_trees(agents: &[RunningAgent]) -> Option<Vec<Vec<JobProcess>>> {
    if agents.is_empty() {
        return Some(Vec::new());
    }
    let everything = process::list_processes()
        .await
        .inspect_err(|error| tracing::debug!(error = format!("{error:#}"), "couldn't list processes"))
        .ok()?;
    Some(agents.iter().map(|agent| started_by(agent, &everything)).collect())
}

/// What one agent started, for a stop: unlike a scan's read, a failed one is an error.
#[cfg(unix)]
async fn agent_tree(agent: &RunningAgent) -> Result<Vec<JobProcess>> {
    Ok(started_by(agent, &process::list_processes().await?))
}

#[cfg(unix)]
fn started_by(agent: &RunningAgent, everything: &[JobProcess]) -> Vec<JobProcess> {
    agent.acp.pid().map_or_else(Vec::new, |leader| process::descendants(everything, leader))
}

/// Everything in each of `agents`' jobs, its own processes too: [`servers_in`] leaves those out by name. A failed read
/// shows nothing.
#[cfg(windows)]
async fn agent_trees(agents: &[RunningAgent]) -> Option<Vec<Vec<JobProcess>>> {
    let mut trees = Vec::new();
    for agent in agents {
        trees.push(agent_tree(agent).await.unwrap_or_default());
    }
    Some(trees)
}

/// Everything in one agent's job, for a stop: unlike a scan's read, a failed one is an error.
#[cfg(windows)]
async fn agent_tree(agent: &RunningAgent) -> Result<Vec<JobProcess>> {
    agent.acp.job_processes().await
}
