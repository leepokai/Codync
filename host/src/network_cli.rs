//! `codync-host network`: this computer's network of the user's computers (codync-network).

use crate::local_api::{local_call, local_call_for};
use anyhow::Result;
use clap::Subcommand;
use codync_network::AgentKind;
use codync_network::hooks::{HookEvent, hook_input, hook_output};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::io::AsyncReadExt;

mod setup;

#[derive(clap::Args)]
pub struct NetworkArgs {
    /// The running host's port (also after the action).
    #[arg(long, global = true, default_value_t = crate::service::DEFAULT_PORT)]
    port: u16,
    /// Which network, by name, when this computer is in several (for create and join: the new
    /// network's name; join takes the invite's otherwise).
    #[arg(long, global = true)]
    network: Option<String>,
    #[command(subcommand)]
    action: Option<Action>,
}

#[derive(Subcommand)]
enum Action {
    /// This computer and the others, as last heard (the default).
    Status,
    /// Start a network with this computer as its owner; prints the recovery code once.
    Create {
        /// This computer's name in the network (default: its host name).
        name: Option<String>,
        /// How computers reach each other without a direct path: public, private or a relay URL.
        #[arg(long)]
        connector: Option<String>,
    },
    /// A one-time invite for another computer, good for an hour (owner and admins).
    Invite,
    /// Join with an invite from the owner or an admin.
    Join {
        invite: String,
        name: Option<String>,
        #[arg(long)]
        connector: Option<String>,
    },
    /// Ask every computer now.
    Peers,
    /// Stop being in a network on this computer (the others list it until it is removed there).
    Leave,
    /// Take a computer out of the network for good.
    Remove {
        computer: String,
        /// Also void every computer it admitted (for a stolen one).
        #[arg(long)]
        cascade: bool,
    },
    /// Make a computer an admin: it can invite and remove members (owner).
    Promote { computer: String },
    /// Make this computer the owner with the network's recovery code.
    Recover { code: String },
    /// Run by a coding agent's hook: tells its session, at its next step, that letters wait for it.
    Hook { agent: HookAgent },
    /// Gives the coding agents installed here (or the ones named) a Network mailbox: adds the
    /// `codync` MCP server, and the hooks that tell a session of new letters, to each one's config.
    Setup {
        agents: Vec<setup::SetupAgent>,
        /// Take them out again (only what this command added).
        #[arg(long)]
        remove: bool,
    },
}

/// The agents whose hooks name their sessions the way their MCP servers do (checked against each).
#[derive(Clone, Copy, clap::ValueEnum)]
enum HookAgent {
    Codex,
    Copilot,
    Grok,
}

impl HookAgent {
    fn kind(self) -> AgentKind {
        match self {
            Self::Codex => AgentKind::Codex,
            Self::Copilot => AgentKind::Copilot,
            Self::Grok => AgentKind::Grok,
        }
    }
}

pub async fn run(args: NetworkArgs) -> Result<()> {
    let port = args.port;
    let network = args.network;
    match args.action.unwrap_or(Action::Status) {
        Action::Status => print_status(&local_call(port, "networkStatus", json!({})).await?),
        Action::Create { name, connector } => {
            let body = json!({"name": name, "connector": connector, "network": network});
            let v = local_call(port, "networkCreate", body).await?;
            println!("Created a network with {} as its owner.", v["computer"].as_str().unwrap_or("this computer"));
            println!("Recovery code (shown once; keep it in a password manager: it makes another computer the owner");
            println!("if this one is lost):\n  {}", v["recoveryCode"].as_str().unwrap_or_default());
            println!(
                "Add a computer: `codync-host network invite` here, then `codync-host network join <invite>` there."
            );
        }
        Action::Invite => {
            let v = local_call(port, "networkInvite", json!({"network": network})).await?;
            println!("{}", v["invite"].as_str().unwrap_or_default());
            println!(
                "One use, for an hour, while this computer stays on: `codync-host network join '<invite>'` on the other one."
            );
        }
        Action::Join { invite, name, connector } => {
            let body = json!({"invite": invite, "name": name, "connector": connector, "network": network});
            print_status(&local_call(port, "networkJoin", body).await?);
        }
        Action::Peers => print_status(&local_call(port, "networkPeers", json!({"network": network})).await?),
        Action::Leave => {
            print_status(&local_call(port, "networkLeave", json!({"network": network})).await?);
        }
        Action::Remove { computer, cascade } => {
            let body = json!({"computer": computer, "cascade": cascade, "network": network});
            local_call(port, "networkRemove", body).await?;
            println!("Removed {computer}.");
        }
        Action::Promote { computer } => {
            local_call(port, "networkPromote", json!({"computer": computer, "network": network})).await?;
            println!("{computer} is an admin.");
        }
        Action::Hook { agent } => hook(port, agent.kind()).await,
        Action::Setup { agents, remove } => setup::run(port, agents, remove).await?,
        Action::Recover { code } => {
            local_call(port, "networkRecover", json!({"code": code, "network": network})).await?;
            println!("This computer is the owner now. Remove the lost one with `codync-host network remove <name>`.");
        }
    }
    Ok(())
}

/// The longest a hook waits for its stdin and the host's answer, and again for its confirmation.
const HOOK_WAIT: Duration = Duration::from_secs(2);
/// The most of a hook's stdin read (a tool's whole output can be on it).
const HOOK_STDIN_MAX: u64 = 4 << 20;

/// Prints what the agent reads, or nothing. Never fails the agent's step: a host that's down, an
/// event it doesn't answer, stdin it can't read (or that stays open) all print nothing.
async fn hook(port: u16, kind: AgentKind) {
    // A bot's own agent: its mail comes through the host, not this.
    if std::env::var_os(crate::mcp::BOT_ENV).is_some() {
        return;
    }
    let ask = async {
        let mut stdin = String::new();
        tokio::io::stdin().take(HOOK_STDIN_MAX).read_to_string(&mut stdin).await.ok()?;
        let (session, event) = serde_json::from_str(&stdin).ok().as_ref().and_then(hook_input)?;
        let answer =
            local_call_for(port, "networkHook", json!({"kind": kind, "session": session}), HOOK_WAIT).await.ok()?;
        Some((session, event, answer))
    };
    let Ok(Some((session, event, answer))) = tokio::time::timeout(HOOK_WAIT, ask).await else { return };
    let (Some(notice), Some(letter)) = (answer["notice"].as_str(), answer["letter"].as_str()) else { return };
    let told = json!({"kind": kind, "session": session, "told": letter});
    let confirm = local_call_for(port, "networkHook", told, HOOK_WAIT);
    match event {
        // A `Stop` that blocks keeps the turn going: only once the host saved the notice as told, so
        // a host that can't save it, or letters read or told meanwhile, let the turn end.
        HookEvent::Stop => {
            if confirm.await.is_ok_and(|v| v["saved"] == true) {
                println!("{}", hook_output(kind, event, notice));
            }
        }
        // After a tool: told first; unconfirmed, the next hook tells it again, which costs nothing.
        HookEvent::PostToolUse => {
            println!("{}", hook_output(kind, event, notice));
            let _ = confirm.await;
        }
    }
}

fn print_status(v: &Value) {
    let networks = v["networks"].as_array().map_or(&[][..], Vec::as_slice);
    if networks.is_empty() {
        println!("This computer isn't in a network. Create one (`codync-host network create`) or join with an invite.");
        return;
    }
    for n in networks {
        let text = |key: &str| n[key].as_str().unwrap_or_default();
        println!("{}: {} ({}) · connector {}", text("name"), text("computer"), text("role"), text("connector"));
        for c in n["computers"].as_array().into_iter().flatten() {
            let state = if c["me"] == true {
                "this computer"
            } else if c["online"] == true {
                "online"
            } else {
                "away"
            };
            let bots: Vec<&str> =
                c["agents"].as_array().into_iter().flatten().filter_map(|a| a["id"].as_str()).collect();
            let name = c["name"].as_str().unwrap_or_default();
            let role = c["role"].as_str().unwrap_or_default();
            println!("  {name:<20} {role:<7} {state:<14} {}", bots.join(", "));
        }
    }
}
