//! `codync-host import`: bots from another app's export file (see `import`).

use crate::local_api::local_call_for;
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Duration;

#[derive(clap::Args)]
pub struct ImportArgs {
    /// The export file (.zip).
    #[arg(required_unless_present = "skill")]
    file: Option<PathBuf>,
    /// Only list what the file holds.
    #[arg(long)]
    dry_run: bool,
    /// Import only these agents (their keys, as `--dry-run` lists them), comma-separated.
    #[arg(long, value_delimiter = ',')]
    only: Option<Vec<String>>,
    /// Print the skill that writes an export file in Grok Bot, and nothing else.
    #[arg(long)]
    skill: bool,
    #[arg(long, default_value_t = crate::service::DEFAULT_PORT)]
    port: u16,
}

pub async fn run(args: ImportArgs) -> Result<()> {
    if args.skill {
        print!("{}", crate::import::EXPORT_SKILL);
        return Ok(());
    }
    let file = args.file.context("name the export file")?;
    let path = std::fs::canonicalize(&file).with_context(|| format!("there's no file at {}", file.display()))?;
    let path = path.to_string_lossy();
    let timeout = Duration::from_mins(10);
    if args.dry_run {
        let v = local_call_for(args.port, "previewImport", json!({"path": path}), timeout).await?;
        print_plan(&v["plan"], &v["skipped"]);
        return Ok(());
    }
    let body = json!({"path": path, "bots": args.only});
    let v = local_call_for(args.port, "importBots", body, timeout).await?;
    for bot in list(&v["bots"]) {
        let n = |k: &str| bot[k].as_u64().unwrap_or(0);
        println!(
            "Imported {}: {} skills, {} routines, {} memories, {} files",
            text(bot, "name"),
            n("skills"),
            n("routines"),
            n("memories"),
            n("files")
        );
    }
    for group in list(&v["groups"]) {
        println!("Imported the group {}", text(group, "name"));
    }
    warnings(&v["warnings"]);
    Ok(())
}

fn print_plan(plan: &Value, skipped: &Value) {
    println!("From {}:", plan["source"].as_str().filter(|s| !s.is_empty()).unwrap_or("an export"));
    for bot in list(&plan["bots"]) {
        let n = |k: &str| bot[k].as_array().map_or(0, Vec::len);
        println!(
            "  {:<24} {}  ({} skills, {} routines, {} memories{})",
            text(bot, "key"),
            text(bot, "name"),
            n("skills"),
            n("routines"),
            n("memories"),
            if bot["files"].is_string() { ", files" } else { "" }
        );
    }
    for group in list(&plan["groups"]) {
        println!("  group {:<18} {}", text(group, "key"), text(group, "name"));
    }
    for path in list(skipped) {
        println!("Left out of the file: {}", path.as_str().unwrap_or_default());
    }
    warnings(&plan["warnings"]);
}

fn warnings(v: &Value) {
    for w in list(v) {
        println!("Note: {}", w.as_str().unwrap_or_default());
    }
}

fn list(v: &Value) -> &[Value] {
    v.as_array().map_or(&[], Vec::as_slice)
}

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or_default()
}
