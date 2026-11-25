//! `codync-host agent`: find or set the CLI for an agent installed somewhere PATH doesn't reach.

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use codync_agent::backends;
use std::path::PathBuf;

#[derive(Args)]
pub struct AgentArgs {
    /// Harness id (for example `codex`).
    pub backend: String,
    #[command(subcommand)]
    pub action: Option<AgentAction>,
}

#[derive(Subcommand)]
pub enum AgentAction {
    /// Save this agent's CLI path.
    Path {
        /// The CLI executable (for example the `codex` printed by `command -v codex`).
        path: PathBuf,
    },
    /// Forget the saved path and return to automatic detection.
    Clear,
}

pub async fn run(backend: &str, action: Option<AgentAction>) -> Result<()> {
    let h = backends::harness(backend).with_context(|| format!("unknown agent {backend}"))?;
    anyhow::ensure!(backends::cli_path_overridable(h.id), "{} doesn't have a selectable CLI path.", h.name);
    match action {
        Some(AgentAction::Path { path }) => {
            let saved = backends::set_cli_path(h.id, &path)?;
            println!("{} now uses {}", h.name, saved.display());
        }
        Some(AgentAction::Clear) => {
            if backends::clear_cli_path(h.id)? {
                println!("{} now uses automatic detection.", h.name);
            } else {
                println!("{} already uses automatic detection.", h.name);
            }
        }
        None => match backends::configured_cli_path(h.id) {
            Some(path) => {
                println!("{}", path.display());
                println!("(saved path; `codync-host agent {backend} clear` restores automatic detection)");
            }
            None if backends::installed(h.id) => {
                let info = backends::install_info(h.id).await;
                println!(
                    "{} is detected automatically at {}.",
                    h.name,
                    info["cli"]["path"].as_str().unwrap_or("its configured location")
                );
            }
            None => bail!(
                "{} isn't detected. Run `codync-host agent {backend} path <path>` with the output of `command -v {}`.",
                h.name,
                h.bins.first().copied().unwrap_or(backend)
            ),
        },
    }
    Ok(())
}
