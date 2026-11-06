//! `codync-host send`: messages a bot through the running host, optionally waiting for its reply, without owning or
//! stopping the bot's work.

mod events;
mod wait;

use crate::tui::net::Client;
use anyhow::Result;
use events::BotFeed;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::Read;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(clap::Args)]
#[command(
    after_long_help = "Exit codes:\n  0  Completed (or accepted, without --wait)\n  1  Stopped, limited, declined, or failed\n  2  Invalid arguments, missing bot, ambiguous name, or --wait on a group chat\n  3  Host unavailable or API request refused\n  4  Timed out; the bot is not stopped\n\nJSON is emitted after a message is accepted. Earlier errors go to stderr."
)]
pub struct SendArgs {
    /// Bot ID, or an exact name (case-insensitive; duplicate names require an ID).
    #[arg(long, value_name = "ID-OR-NAME")]
    bot: String,
    /// Read a UTF-8 message file; use - to read stdin instead.
    #[arg(long, value_name = "PATH", conflicts_with = "text")]
    file: Option<PathBuf>,
    /// Message words, joined by spaces; mutually exclusive with --file.
    #[arg(value_name = "TEXT", num_args = 1.., required_unless_present = "file", conflicts_with = "file")]
    text: Vec<String>,
    /// Wait for the turn that answers the message to end, and print its reply (not for group chats).
    #[arg(long)]
    wait: bool,
    /// Active wait limit in seconds; time waiting for approval is excluded.
    #[arg(long, default_value_t = 1800, value_parser = clap::value_parser!(u64).range(1..), value_name = "SECS")]
    timeout: u64,
    /// Print one JSON object instead of plain text.
    #[arg(long)]
    json: bool,
    /// Quote-reply to this message of the bot's chat: an `entryId` or `replyId` from an earlier --json result.
    #[arg(long, value_name = "ENTRY-ID")]
    reply_to: Option<String>,
    /// Local host port (development builds use their own default).
    #[arg(long, default_value_t = crate::service::DEFAULT_PORT)]
    port: u16,
}

/// The exit codes `--help` documents.
#[derive(Clone, Copy)]
enum Exit {
    Completed = 0,
    Failed = 1,
    Usage = 2,
    Unavailable = 3,
    Timeout = 4,
}

/// Why the command ends before it has a result to print.
struct Failure {
    exit: Exit,
    message: String,
}

impl Failure {
    /// The host can't be reached, or refused a request.
    fn host(message: impl Into<String>) -> Self {
        Self { exit: Exit::Unavailable, message: message.into() }
    }

    fn usage(message: impl Into<String>) -> Self {
        Self { exit: Exit::Usage, message: message.into() }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum Status {
    Completed,
    Failed,
    Timeout,
}

/// Why a turn ended: the agent's stop reason as the host's notices carry it (`stopReason`).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum StopReason {
    EndTurn,
    Cancelled,
    MaxTokens,
    MaxTurnRequests,
    Refusal,
    StartFailed,
    Error,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    bot_id: String,
    entry_id: String,
    entry_seq: i64,
    turn: i64,
    status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_reason: Option<StopReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reply: Option<String>,
    /// The reply's last message, which `--reply-to` can quote.
    #[serde(skip_serializing_if = "Option::is_none")]
    reply_id: Option<String>,
    notices: Vec<String>,
    elapsed_seconds: f64,
    waited_for_approval_seconds: f64,
}

impl Report {
    fn accepted(bot: &str, entry: &Value, started: Instant) -> Result<Self, Failure> {
        let invalid = || Failure::host("the host returned an invalid message");
        Ok(Self {
            bot_id: bot.to_owned(),
            entry_id: entry["id"].as_str().ok_or_else(invalid)?.to_owned(),
            entry_seq: entry["seq"].as_i64().ok_or_else(invalid)?,
            turn: entry["turn"].as_i64().ok_or_else(invalid)?,
            status: Status::Completed,
            stop_reason: None,
            reply: None,
            reply_id: None,
            notices: Vec::new(),
            elapsed_seconds: started.elapsed().as_secs_f64(),
            waited_for_approval_seconds: 0.0,
        })
    }

    /// Records a notice and says it on stderr.
    fn note(&mut self, text: String) {
        eprintln!("{text}");
        self.notices.push(text);
    }

    fn emit(&self, args: &SendArgs, token: &str) -> Exit {
        if args.json {
            let Ok(text) = serde_json::to_string(self) else {
                eprintln!("couldn't encode the send result");
                return Exit::Failed;
            };
            println!("{}", redact(&text, token));
        } else if args.wait {
            if let Some(reply) = &self.reply {
                println!("{}", redact(reply.trim_end_matches(['\r', '\n']), token));
            }
        } else {
            println!("id={} seq={} turn={}", self.entry_id, self.entry_seq, self.turn);
        }
        match self.status {
            Status::Completed => Exit::Completed,
            Status::Failed => Exit::Failed,
            Status::Timeout => Exit::Timeout,
        }
    }
}

/// `codync-host send`: exits with the code `--help` documents.
pub async fn run(args: SendArgs) -> Result<()> {
    let exit = execute(&args).await.unwrap_or_else(|e| {
        eprintln!("{}", e.message);
        e.exit
    });
    if !matches!(exit, Exit::Completed) {
        std::process::exit(exit as i32);
    }
    Ok(())
}

async fn execute(args: &SendArgs) -> Result<Exit, Failure> {
    // A file or stdin read blocks: off the async workers.
    let (file, words) = (args.file.clone(), args.text.clone());
    let text = tokio::task::spawn_blocking(move || read_message(file.as_deref(), &words))
        .await
        .map_err(|_| Failure::usage("couldn't read the message"))??;
    let base = format!("http://127.0.0.1:{}", args.port);
    let token = Client::new(&base, None).token().ok_or_else(|| Failure::host(crate::NOT_RUNNING))?;
    let mut feed = BotFeed::connect(&base, &token).await?;
    let bots = feed.bots();
    let bot = resolve_bot(&args.bot, &bots)?;
    if args.wait && bot["kind"] == "group" {
        return Err(Failure::usage("--wait doesn't support group chats"));
    }
    let bot = bot["id"].as_str().unwrap_or_default();
    let started = Instant::now();
    if args.wait {
        feed.track(bot);
    }
    let mut body = json!({"botId": bot, "text": text, "clientNonce": uuid::Uuid::new_v4().to_string()});
    if let Some(id) = &args.reply_to {
        body["replyTo"] = id.as_str().into();
    }
    let answer = send(&base, &token, &body).await?;
    let mut report = Report::accepted(bot, &answer["entry"], started)?;
    if args.wait
        && let Err(e) = wait::reply(args, &token, &mut feed, &mut report).await
    {
        // The message was accepted: the result still says which one, and why the wait ended.
        let (elapsed, approval) = feed.timing();
        report.elapsed_seconds = elapsed.as_secs_f64();
        report.waited_for_approval_seconds = approval.as_secs_f64();
        report.status = Status::Failed;
        report.stop_reason = Some(StopReason::Error);
        report.note(redact(&e.message, &token));
        report.emit(args, &token);
        return Ok(e.exit);
    }
    Ok(report.emit(args, &token))
}

/// Sends the message. The host keeps one message per `clientNonce`, so one whose answer never came back (the host was
/// restarting, or the connection dropped) is sent again with the same nonce; one the host refused is not.
async fn send(base: &str, token: &str, body: &Value) -> Result<Value, Failure> {
    let mut attempts = 1;
    loop {
        let sent = crate::http()
            .post(format!("{base}/api/send"))
            .bearer_auth(token)
            .json(body)
            .timeout(Duration::from_secs(20))
            .send()
            .await;
        let answer = match sent {
            Ok(res) if !res.status().is_success() => {
                let refused: Value = res.json().await.unwrap_or_default();
                return Err(Failure::host(redact(refused["error"].as_str().unwrap_or("the host refused"), token)));
            }
            Ok(res) => res.json::<Value>().await.ok(),
            Err(_) => None,
        };
        match answer {
            Some(answer) => return Ok(answer),
            None if attempts == 3 => return Err(Failure::host(crate::NOT_RUNNING)),
            None => attempts += 1,
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

fn read_message(file: Option<&std::path::Path>, words: &[String]) -> Result<String, Failure> {
    let message = match file {
        Some(path) if path.as_os_str() == "-" => {
            let mut text = String::new();
            std::io::stdin()
                .read_to_string(&mut text)
                .map_err(|_| Failure::usage("couldn't read a UTF-8 message from stdin"))?;
            text
        }
        Some(path) => {
            std::fs::read_to_string(path).map_err(|_| Failure::usage("couldn't read the UTF-8 message file"))?
        }
        None => words.join(" "),
    };
    if message.trim().is_empty() {
        return Err(Failure::usage("a nonempty message is required"));
    }
    Ok(message)
}

/// The bot `query` names: by ID, or else by its exact name ignoring case when no other bot has that name.
fn resolve_bot<'a>(query: &str, bots: &'a [Value]) -> Result<&'a Value, Failure> {
    if let Some(bot) = bots.iter().find(|bot| bot["id"] == query) {
        return Ok(bot);
    }
    let name = query.to_lowercase();
    let named: Vec<&Value> =
        bots.iter().filter(|b| b["name"].as_str().is_some_and(|n| n.to_lowercase() == name)).collect();
    match named.as_slice() {
        [] => Err(Failure::usage("no bot matches that ID or name")),
        [bot] => Ok(bot),
        _ => {
            let ids: Vec<&str> = named.iter().filter_map(|b| b["id"].as_str()).collect();
            Err(Failure::usage(format!("ambiguous bot name; use an ID: {}", ids.join(", "))))
        }
    }
}

/// `text` without this computer's API token, which nothing the command prints may reveal.
fn redact(text: &str, token: &str) -> String {
    text.replace(token, "[redacted]")
}
