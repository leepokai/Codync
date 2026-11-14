//! The standalone daemon and command line: one running node per computer, which agents and
//! people on that computer drive with one-shot commands.

mod control;

use anyhow::{Context, Result, anyhow, bail};
use codync_network::{Access, Address, Agent, AgentKind, Connector, Letter, Node, Reach, Rule, Sent};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, BufReader};

const USAGE: &str = "usage: codync-network [--dir DIR] [--connector public|private|RELAY_URL] <command>
start this computer's node (keep it running, e.g. as a service):
  init <name>             start a network with this computer as its owner
  join <invite> <name>    join with an invite from another computer
  up                      start (with the network this computer is in, if any)
then, from any terminal or agent on this computer:
  invite                  a one-time invite for another computer (owner and admins)
  peers                   the computers, their roles and agents
  send <agent[@computer]> <text> [--from AGENT] [--notice] [--reply-to ID]
  agent <id> claude <session-id> | codex <thread-id> [--about TEXT]
  agent <id> --remove     stop offering an agent
  agents                  this computer's agents
  promote <computer>      make a computer an admin (owner)
  remove <computer> [--cascade]  take a computer out for good (--cascade: and what it admitted)
  recover <code>          make this computer the owner with the recovery code
  rules                   who on the other computers may write to agents here (first match decides)
  rules allow|deny <agent@computer> <agent> [--first]  add a rule (* matches any; --first puts it on top)
  rules remove <n>        take out rule n";

/// The agent a person at this computer's terminal speaks as.
const TERMINAL: &str = "terminal";
const AGENTS_FILE: &str = "agents.json";
const RULES_FILE: &str = "rules.json";

#[derive(Clone, Serialize, Deserialize)]
struct LocalAgent {
    #[serde(flatten)]
    agent: Agent,
    reach: Reach,
}

struct Daemon {
    node: Node,
    dir: PathBuf,
    agents: Mutex<Vec<LocalAgent>>,
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("codync-network: {e:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let dir = take_flag(&mut args, "--dir")?
        .map(PathBuf::from)
        .or_else(default_dir)
        .ok_or_else(|| anyhow!("no home folder; pass --dir"))?;
    let connector: Connector = take_flag(&mut args, "--connector")?.as_deref().unwrap_or("public").parse()?;
    match args.first().map(String::as_str) {
        None | Some("help" | "--help" | "-h") => {
            println!("{USAGE}");
            Ok(())
        }
        Some("init" | "join" | "up") => daemon(&dir, &connector, &args).await,
        Some(_) => {
            println!("{}", control::ask(&dir, &args).await?);
            Ok(())
        }
    }
}

async fn daemon(dir: &Path, connector: &Connector, args: &[String]) -> Result<()> {
    let (node, mut inbox) = Node::start(dir, connector).await?;
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["init", name] => {
            let code = node.create(name)?;
            println!(
                "Recovery code (shown once; keep it in a password manager, it moves the owner if this computer is lost):\n  {code}"
            );
        }
        ["join", invite, name] => node.join(invite, name).await?,
        ["up"] => {}
        _ => bail!("{USAGE}"),
    }
    // Not in a network yet: this computer's own agents still reach each other.
    let name = node.name().unwrap_or_else(|| "this computer (no network)".into());
    // The agents this node knocks on answer with this same program, and this folder when it isn't the default.
    if let Ok(exe) = std::env::current_exe() {
        let dir_flag =
            if default_dir().as_deref() == Some(dir) { String::new() } else { format!(" --dir '{}'", dir.display()) };
        node.set_reply_command(format!("'{}'{dir_flag}", exe.display()));
    }
    node.set_rules(load(dir, RULES_FILE)?)?;
    let daemon = Arc::new(Daemon { node, dir: dir.to_path_buf(), agents: Mutex::new(load(dir, AGENTS_FILE)?) });
    daemon.offer();
    control::serve(dir, daemon.clone())?;
    println!("{name} is on ({}). Commands work here and from other terminals.", daemon.node.id().fmt_short());

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut typing = true;
    loop {
        tokio::select! {
            Some(m) = inbox.recv() => {
                let kind = if m.wake { "" } else { " (notice)" };
                println!("← {}{kind} [{}]: {}", m.from, m.id, m.body);
            }
            line = lines.next_line(), if typing => match line {
                Ok(Some(line)) if line.trim() == "quit" => break,
                Ok(Some(line)) => {
                    let words: Vec<String> = line.split_whitespace().map(String::from).collect();
                    match daemon.command(&words).await {
                        Ok(out) => println!("{out}"),
                        Err(e) => println!("! {e:#}"),
                    }
                }
                // No terminal (a service): keep running on the control socket alone.
                _ => typing = false,
            },
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    Ok(())
}

impl Daemon {
    /// Runs one command, typed here or sent by `codync-network <command>`.
    async fn command(&self, words: &[String]) -> Result<String> {
        let mut args = words.to_vec();
        let from = take_flag(&mut args, "--from")?.unwrap_or_else(|| TERMINAL.into());
        let reply_to = take_flag(&mut args, "--reply-to")?;
        let about = take_flag(&mut args, "--about")?;
        let notice = take_switch(&mut args, "--notice");
        let remove = take_switch(&mut args, "--remove");
        let cascade = take_switch(&mut args, "--cascade");
        let first = take_switch(&mut args, "--first");
        match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
            ["invite"] => self.node.invite(),
            ["peers"] => self.peers().await,
            ["send", to, ref text @ ..] if !text.is_empty() => {
                let to: Address = self.node.resolve(to)?;
                let mut letter = Letter::new(from, to.clone(), text.join(" "));
                if notice {
                    letter = letter.notice();
                }
                if let Some(id) = reply_to {
                    letter = letter.reply_to(id);
                }
                Ok(match self.node.send(letter).await? {
                    Sent::Delivered => format!("delivered to {to}"),
                    Sent::Queued => format!("{} is away; it gets this when it's back", to.computer),
                })
            }
            ["agent", id] if remove => self.forget(id),
            ["agent", id, kind, handle] => {
                let reach = match kind {
                    "claude" => Reach::ClaudeCode { session: handle.into() },
                    "codex" => Reach::Codex { thread: handle.into() },
                    _ => bail!("an agent is reached through claude or codex"),
                };
                let kind = reach.kind().unwrap_or_default();
                let agent = Agent { id: id.into(), name: id.into(), about: about.unwrap_or_default(), kind };
                self.add(LocalAgent { agent, reach })
            }
            ["agents"] => Ok(self.listing()),
            ["remove", computer] => {
                self.node.remove(computer, cascade)?;
                Ok(format!("removed {computer}"))
            }
            ["promote", computer] => {
                self.node.promote(computer)?;
                Ok(format!("{computer} is an admin"))
            }
            ["rules"] => Ok(self.rules_listing()),
            ["rules", "remove", n] => {
                let mut rules = self.node.rules();
                let i = n
                    .parse::<usize>()
                    .ok()
                    .filter(|i| (1..=rules.len()).contains(i))
                    .ok_or_else(|| anyhow!("no rule {n}"))?;
                rules.remove(i - 1);
                self.set_rules(rules)
            }
            ["rules", access, from, to] => {
                let rule = Rule::new(access.parse::<Access>()?, from, to)?;
                let mut rules = self.node.rules();
                if first {
                    rules.insert(0, rule);
                } else {
                    rules.push(rule);
                }
                self.set_rules(rules)
            }
            ["recover", ref code @ ..] if !code.is_empty() => {
                self.node.recover(&code.join(""))?;
                Ok("this computer is the owner now; remove the lost one".into())
            }
            _ => bail!("{USAGE}"),
        }
    }

    fn set_rules(&self, rules: Vec<Rule>) -> Result<String> {
        codync_network::rules::check_all(&rules)?;
        save(&self.dir, RULES_FILE, &rules)?;
        self.node.set_rules(rules)?;
        Ok(self.rules_listing())
    }

    fn rules_listing(&self) -> String {
        let rules = self.node.rules();
        if rules.is_empty() {
            return "no rules: every computer in the network may write to every agent here".into();
        }
        rules.iter().enumerate().map(|(i, r)| format!("{:>3}  {r}", i + 1)).collect::<Vec<_>>().join("\n")
    }

    async fn peers(&self) -> Result<String> {
        let mut out = Vec::new();
        for p in self.node.peers().await? {
            let agents: Vec<&str> = p.agents.iter().map(|a| a.id.as_str()).collect();
            let state = if p.me {
                "this computer"
            } else if p.online {
                "online"
            } else {
                "away"
            };
            let role = format!("{:?}", p.role).to_lowercase();
            out.push(format!("{:<16} {role:<7} {state:<14} {}", p.name, agents.join(", ")));
        }
        Ok(out.join("\n"))
    }

    fn add(&self, local: LocalAgent) -> Result<String> {
        if local.agent.id == TERMINAL
            || local.agent.id.contains(['/', ' '])
            || local.agent.id.len() > codync_network::MAX_ADDRESS
        {
            bail!("pick another agent name (not {TERMINAL}, shorter, no spaces or /)");
        }
        let id = local.agent.id.clone();
        let mut agents = self.agents.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        agents.retain(|a| a.agent.id != id);
        agents.push(local);
        save(&self.dir, AGENTS_FILE, &agents)?;
        drop(agents);
        self.offer();
        Ok(format!("{id} is on the network"))
    }

    fn forget(&self, id: &str) -> Result<String> {
        let mut agents = self.agents.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = agents.len();
        agents.retain(|a| a.agent.id != id);
        if agents.len() == before {
            bail!("no agent called {id} on this computer");
        }
        save(&self.dir, AGENTS_FILE, &agents)?;
        drop(agents);
        self.offer();
        Ok(format!("{id} is off the network"))
    }

    fn listing(&self) -> String {
        let agents = self.agents.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut out = vec![format!("{TERMINAL:<16} this terminal")];
        for a in agents.iter() {
            let how = match &a.reach {
                Reach::ClaudeCode { session } => format!("Claude Code {session}"),
                Reach::Codex { thread } => format!("Codex {thread}"),
                Reach::App => "app".into(),
            };
            out.push(format!("{:<16} {how}", a.agent.id));
        }
        out.join("\n")
    }

    /// Tells the node what this computer offers: the terminal, then every registered agent.
    fn offer(&self) {
        let terminal = Agent {
            id: TERMINAL.into(),
            name: "Terminal".into(),
            about: "A person at a terminal".into(),
            kind: AgentKind::Terminal,
        };
        let agents = self.agents.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let offered = agents.iter().map(|a| (a.agent.clone(), a.reach.clone()));
        self.node.set_agents(std::iter::once((terminal, Reach::App)).chain(offered).collect::<Vec<_>>());
    }
}

/// Its own folder: a Codync host on the same computer keeps its node in `~/.codync/network`, and
/// two nodes with one key would each take the other's letters.
fn default_dir() -> Option<PathBuf> {
    std::env::home_dir().map(|h| h.join(".codync-network"))
}

/// A list kept in the node's folder (`agents.json`, `rules.json`); none yet is empty.
fn load<T: serde::de::DeserializeOwned>(dir: &Path, file: &str) -> Result<Vec<T>> {
    match std::fs::read(dir.join(file)) {
        Ok(bytes) => serde_json::from_slice(&bytes).with_context(|| format!("{file} is damaged")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e).with_context(|| format!("couldn't read {file}")),
    }
}

fn save<T: Serialize>(dir: &Path, file: &str, items: &[T]) -> Result<()> {
    std::fs::write(dir.join(file), serde_json::to_vec_pretty(items)?).with_context(|| format!("couldn't save {file}"))
}

fn take_flag(args: &mut Vec<String>, flag: &str) -> Result<Option<String>> {
    let Some(at) = args.iter().position(|a| a == flag) else { return Ok(None) };
    if at + 1 >= args.len() {
        bail!("{flag} needs a value");
    }
    let value = args.remove(at + 1);
    args.remove(at);
    Ok(Some(value))
}

fn take_switch(args: &mut Vec<String>, switch: &str) -> bool {
    let before = args.len();
    args.retain(|a| a != switch);
    args.len() != before
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_are_kept_beside_the_agents() {
        let dir = std::env::temp_dir().join(format!("codync-network-rules-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temp folder");
        assert_eq!(load::<Rule>(&dir, RULES_FILE).expect("none yet"), []);
        let rules = vec![Rule::new(Access::Deny, "*@server", "*").expect("a rule")];
        save(&dir, RULES_FILE, &rules).expect("saved");
        assert_eq!(load::<Rule>(&dir, RULES_FILE).expect("read back"), rules);
        let _ = std::fs::remove_dir_all(dir);
    }
}
