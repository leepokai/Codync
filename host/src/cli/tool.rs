//! Source reference: Hermes agent tool RPC / `batch_runner.py` (MIT, 10d25f8a).
//! Adaptation: scripts reuse the host's authenticated MCP call path without an LLM turn.

use crate::mcp;
use anyhow::{Context, Result, ensure};
use clap::{Args as ClapArgs, ValueEnum};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::PathBuf;

const MAX_ARGUMENTS: u64 = 256 * 1024;

#[derive(Clone, Copy, ValueEnum)]
enum Server {
    Chat,
    Connectors,
    Routines,
    Computer,
    Team,
    Manage,
    Memory,
}

impl Server {
    fn mcp(self) -> mcp::Server {
        match self {
            Self::Chat => mcp::Server::Chat,
            Self::Connectors => mcp::Server::Connectors,
            Self::Routines => mcp::Server::Routines,
            Self::Computer => mcp::Server::Computer,
            Self::Team => mcp::Server::Team,
            Self::Manage => mcp::Server::Manage,
            Self::Memory => mcp::Server::Memory,
        }
    }
}

#[derive(ClapArgs)]
#[command(
    after_long_help = "Writes one MCP tool-result JSON object. Exit 0: success; exit 1: tool/runtime/JSON input failure; exit 2: invalid CLI arguments. A failure may follow partial work: do not retry mutating tools blindly. For private arguments prefer --file - over shell arguments."
)]
pub struct Args {
    /// Exact bot ID; all ordinary host tool and approval rules still apply.
    #[arg(long)]
    bot: String,
    /// Built-in MCP tool family.
    #[arg(long, value_enum)]
    server: Server,
    /// Tool name from the selected server's MCP catalog.
    name: String,
    /// JSON object; defaults to {}. Prefer --file for sensitive values.
    #[arg(long, conflicts_with = "file")]
    arguments: Option<String>,
    /// UTF-8 JSON argument file, or - for stdin (at most 256 KiB).
    #[arg(long, conflicts_with = "arguments")]
    file: Option<PathBuf>,
    #[arg(long, default_value_t = crate::service::DEFAULT_PORT)]
    port: u16,
}

fn parse_arguments(bytes: &[u8]) -> Result<Value> {
    ensure!(u64::try_from(bytes.len())? <= MAX_ARGUMENTS, "Tool arguments exceed 256 KiB");
    let value: Value = serde_json::from_slice(bytes).context("Tool arguments must be valid JSON")?;
    ensure!(value.is_object(), "Tool arguments must be a JSON object");
    Ok(value)
}

fn read_arguments(args: &Args) -> Result<Value> {
    if let Some(arguments) = &args.arguments {
        return parse_arguments(arguments.as_bytes());
    }
    let Some(path) = &args.file else { return Ok(json!({})) };
    let mut bytes = Vec::new();
    if path.as_os_str() == "-" {
        std::io::stdin().lock().take(MAX_ARGUMENTS + 1).read_to_end(&mut bytes)?;
    } else {
        std::fs::File::open(path).context("Opening tool arguments")?.take(MAX_ARGUMENTS + 1).read_to_end(&mut bytes)?;
    }
    parse_arguments(&bytes)
}

pub async fn run(args: &Args) -> Result<()> {
    ensure!(!args.bot.trim().is_empty(), "A bot ID is required");
    ensure!(!args.name.trim().is_empty(), "A tool name is required");
    let arguments = read_arguments(args)?;
    let token = std::fs::read_to_string(crate::service::data_dir().join("token"))
        .context("The local host token is unavailable; start the local host first")?;
    ensure!(!token.trim().is_empty(), "The local host token is empty");
    // This is the same operation exposed by the stdio MCP bridge. It deliberately keeps
    // host validation, output limits, approval cards, and the bot's tool policies in one place.
    let result = mcp::call(
        args.port,
        token.trim(),
        &args.bot,
        &json!({"name":args.name,"arguments":arguments}),
        args.server.mcp(),
    )
    .await;
    let mut output = std::io::stdout().lock();
    serde_json::to_writer(&mut output, &result)?;
    writeln!(output)?;
    output.flush()?;
    ensure!(result["isError"] != true, "The tool failed; inspect the JSON result. Partial work may have happened");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hermes_rpc_accepts_only_bounded_json_objects() {
        assert_eq!(parse_arguments(br#"{"query":"recap"}"#).unwrap()["query"], "recap");
        assert!(parse_arguments(b"[]").is_err());
        assert!(parse_arguments(b"null").is_err());
        assert!(parse_arguments(b"not JSON").is_err());
        let oversized = vec![b' '; usize::try_from(MAX_ARGUMENTS).unwrap() + 1];
        assert!(parse_arguments(&oversized).is_err());
    }
}
