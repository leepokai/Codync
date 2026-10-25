//! `codync-host network`: this computer's network of the user's computers (codync-network).

use crate::local_call;
use anyhow::Result;
use clap::Subcommand;
use serde_json::{Value, json};

#[derive(clap::Args)]
pub struct NetworkArgs {
    /// The running host's port (also after the action).
    #[arg(long, global = true, default_value_t = crate::service::DEFAULT_PORT)]
    port: u16,
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
}

pub async fn run(args: NetworkArgs) -> Result<()> {
    let port = args.port;
    match args.action.unwrap_or(Action::Status) {
        Action::Status => print_status(&local_call(port, "networkStatus", json!({})).await?),
        Action::Create { name, connector } => {
            let v = local_call(port, "networkCreate", json!({"name": name, "connector": connector})).await?;
            println!("Created a network with {} as its owner.", v["computer"].as_str().unwrap_or("this computer"));
            println!("Recovery code (shown once; keep it in a password manager: it makes another computer the owner");
            println!("if this one is lost):\n  {}", v["recoveryCode"].as_str().unwrap_or_default());
            println!(
                "Add a computer: `codync-host network invite` here, then `codync-host network join <invite>` there."
            );
        }
        Action::Invite => {
            let v = local_call(port, "networkInvite", json!({})).await?;
            println!("{}", v["invite"].as_str().unwrap_or_default());
            println!(
                "One use, for an hour, while this computer stays on: `codync-host network join '<invite>'` on the other one."
            );
        }
        Action::Join { invite, name, connector } => {
            let body = json!({"invite": invite, "name": name, "connector": connector});
            print_status(&local_call(port, "networkJoin", body).await?);
        }
        Action::Peers => print_status(&local_call(port, "networkPeers", json!({})).await?),
        Action::Remove { computer, cascade } => {
            local_call(port, "networkRemove", json!({"computer": computer, "cascade": cascade})).await?;
            println!("Removed {computer}.");
        }
        Action::Promote { computer } => {
            local_call(port, "networkPromote", json!({"computer": computer})).await?;
            println!("{computer} is an admin.");
        }
        Action::Recover { code } => {
            local_call(port, "networkRecover", json!({"code": code})).await?;
            println!("This computer is the owner now. Remove the lost one with `codync-host network remove <name>`.");
        }
    }
    Ok(())
}

fn print_status(v: &Value) {
    if v["joined"] != true {
        println!("This computer isn't in a network. Create one (`codync-host network create`) or join with an invite.");
        return;
    }
    let me = v["computer"].as_str().unwrap_or_default();
    let role = v["role"].as_str().unwrap_or_default();
    println!("{me} ({role}) · connector {}", v["connector"].as_str().unwrap_or("public"));
    for c in v["computers"].as_array().into_iter().flatten() {
        let state = if c["me"] == true {
            "this computer"
        } else if c["online"] == true {
            "online"
        } else {
            "away"
        };
        let bots: Vec<&str> = c["bots"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        let name = c["name"].as_str().unwrap_or_default();
        let role = c["role"].as_str().unwrap_or_default();
        println!("  {name:<20} {role:<7} {state:<14} {}", bots.join(", "));
    }
}
