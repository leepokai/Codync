//! Send through the local host without owning or stopping the bot's work.

mod wait;

use super::api::Api;
use super::events::BotFeed;
use clap::Args as ClapArgs;
use serde::Serialize;
use serde_json::{Value, json};
use std::io::Read;
use std::path::PathBuf;
use std::time::Instant;

#[derive(ClapArgs)]
#[command(
    after_long_help = "Exit codes:\n  0  Completed (or accepted, without --wait)\n  1  Stopped, limited, declined, or failed\n  2  Invalid arguments, missing bot, or ambiguous name\n  3  Host unavailable or API request refused\n  4  Timed out; the bot is not stopped\n\nJSON is emitted after a message is accepted. Earlier errors go to stderr."
)]
pub struct Args {
    /// Bot ID, or an exact name (case-insensitive; duplicate names require an ID).
    #[arg(long, value_name = "ID-OR-NAME")]
    bot: String,
    /// Read a UTF-8 message file; use - to read stdin instead.
    #[arg(long, value_name = "PATH", conflicts_with = "text")]
    file: Option<PathBuf>,
    /// Message words, joined by spaces; mutually exclusive with --file.
    #[arg(value_name = "TEXT", num_args = 1.., required_unless_present = "file", conflicts_with = "file")]
    text: Vec<String>,
    /// Wait for the final reply and for its turn to settle.
    #[arg(long)]
    wait: bool,
    /// Active wait limit in seconds; time waiting for approval is excluded.
    #[arg(long, default_value_t = 1800, value_parser = clap::value_parser!(u64).range(1..), value_name = "SECS")]
    timeout: u64,
    /// Print one JSON object instead of plain text.
    #[arg(long)]
    json: bool,
    /// Reply in the thread rooted at this entry ID.
    #[arg(long, value_name = "ENTRY-ID")]
    thread: Option<String>,
    /// Local host port (development builds use their own default).
    #[arg(long, default_value_t = crate::service::DEFAULT_PORT)]
    port: u16,
}

pub(super) struct Failure {
    pub code: i32,
    pub message: String,
}

impl Failure {
    pub fn api(message: impl Into<String>) -> Self {
        Self { code: 3, message: message.into() }
    }

    fn argument(message: impl Into<String>) -> Self {
        Self { code: 2, message: message.into() }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum Status {
    Completed,
    Failed,
    Timeout,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    bot_id: String,
    entry_seq: i64,
    turn: i64,
    status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reply: Option<String>,
    notices: Vec<String>,
    elapsed_seconds: f64,
    waited_for_approval_seconds: f64,
}

impl Report {
    fn accepted(bot: &str, entry: &Value, started: Instant) -> Result<Self, Failure> {
        if entry["id"].as_str().is_none() {
            return Err(Failure::api("the host returned an invalid message"));
        }
        Ok(Self {
            bot_id: bot.to_owned(),
            entry_seq: entry["seq"].as_i64().ok_or_else(|| Failure::api("the host returned an invalid message"))?,
            turn: entry["turn"].as_i64().ok_or_else(|| Failure::api("the host returned an invalid turn"))?,
            status: Status::Completed,
            stop_reason: None,
            reply: None,
            notices: Vec::new(),
            elapsed_seconds: started.elapsed().as_secs_f64(),
            waited_for_approval_seconds: 0.0,
        })
    }

    fn emit(&self, args: &Args, api: &Api) -> i32 {
        if args.json {
            let Ok(text) = serde_json::to_string(self) else {
                eprintln!("couldn't encode the send result");
                return 1;
            };
            println!("{}", api.redact(&text));
        } else if args.wait {
            if let Some(reply) = &self.reply {
                println!("{}", api.redact(reply.trim_end_matches(['\r', '\n'])));
            }
        } else {
            println!("seq={} turn={}", self.entry_seq, self.turn);
        }
        match self.status {
            Status::Completed => 0,
            Status::Failed => 1,
            Status::Timeout => 4,
        }
    }
}

pub async fn run(args: Args) -> i32 {
    match execute(&args).await {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{}", e.message);
            e.code
        }
    }
}

async fn execute(args: &Args) -> Result<i32, Failure> {
    let text = read_message(args)?;
    let api = Api::new(args.port)?;
    let mut feed = BotFeed::connect(&api).await?;
    let bot = resolve_bot(&args.bot, &feed.bots())?;
    let started = Instant::now();
    if args.wait {
        feed.track(&bot);
    }
    let mut body = json!({"botId": bot, "text": text, "clientNonce": uuid::Uuid::new_v4().to_string()});
    if let Some(thread) = &args.thread {
        body["threadId"] = thread.clone().into();
    }
    let response = api.send(body).await?;
    let entry = &response["entry"];
    let mut report = Report::accepted(&bot, entry, started)?;
    if args.wait
        && let Err(e) = wait::reply(args, &api, &mut feed, entry, &mut report).await
    {
        let (elapsed, approval) = feed.timing();
        report.elapsed_seconds = elapsed.as_secs_f64();
        report.waited_for_approval_seconds = approval.as_secs_f64();
        report.status = Status::Failed;
        report.stop_reason = Some("error".into());
        report.notices.push(api.redact(&e.message));
        eprintln!("{}", api.redact(&e.message));
        report.emit(args, &api);
        return Ok(e.code);
    }
    Ok(report.emit(args, &api))
}

fn read_message(args: &Args) -> Result<String, Failure> {
    let message = match &args.file {
        Some(path) if path.as_os_str() == "-" => {
            let mut text = String::new();
            std::io::stdin()
                .read_to_string(&mut text)
                .map_err(|_| Failure::argument("couldn't read a UTF-8 message from stdin"))?;
            text
        }
        Some(path) => {
            std::fs::read_to_string(path).map_err(|_| Failure::argument("couldn't read the UTF-8 message file"))?
        }
        None => args.text.join(" "),
    };
    if message.trim().is_empty() {
        return Err(Failure::argument("a nonempty message is required"));
    }
    Ok(message)
}

fn resolve_bot(query: &str, bots: &[Value]) -> Result<String, Failure> {
    if let Some(bot) = bots.iter().find(|bot| bot["id"] == query) {
        return Ok(bot["id"].as_str().expect("the id matched a string").to_owned());
    }
    let name = query.to_lowercase();
    let ids: Vec<_> = bots
        .iter()
        .filter(|b| b["name"].as_str().is_some_and(|n| n.to_lowercase() == name))
        .filter_map(|b| b["id"].as_str())
        .collect();
    match ids.as_slice() {
        [] => Err(Failure::argument("no bot matches that ID or name")),
        [id] => Ok((*id).to_owned()),
        _ => Err(Failure::argument(format!("ambiguous bot name; use an ID: {}", ids.join(", ")))),
    }
}
