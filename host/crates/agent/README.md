# codync-agent

Coding agents on one computer, for an app that runs them. It is the part of [Codync](https://github.com/leepokai/Codync)'s
host that is true of the agents themselves, with no app in it:

- **`backends`** knows the agent CLIs (Claude Code, Codex, OpenCode, Gemini CLI, Cursor, Devin, Pi, …): where each one
  is installed, how to install and update it, how it signs in, and what its exit means when it quits at start.
- **`registry`** reads the [ACP registry](https://agentclientprotocol.com) and resolves an entry to a command, downloading
  its binary or npm package; **`node`** provides a Node.js for npm agents on a computer without one.
- **`acp`** speaks the Agent Client Protocol (JSON-RPC on stdio) to a started agent: what it can do (`Capabilities`),
  a session's controls (`Session`: prompt, cancel, steer) and why a turn ended (`StopReason`), and it tells failures
  apart (signed out, exited, timed out, a model or mode it lacks); **`events`** types what the agent says (below);
  **`process`** owns the process tree each agent starts,
  so stopping the agent stops everything under it (a session of its own on Unix, a job object on Windows).
- **`modes`** reads an agent's permission modes (its Auto among them) and picks the one to switch to.
- **`accounts`** gives an agent more than one login on the same computer (below).

```rust,no_run
use std::path::PathBuf;

// The app's one HTTP client, set up only once a request goes out.
fn http() -> &'static reqwest::Client {
    static CLIENT: std::sync::LazyLock<reqwest::Client> = std::sync::LazyLock::new(reqwest::Client::new);
    &CLIENT
}

// Once at start, before any agent runs.
codync_agent::init(codync_agent::Setup {
    data_dir: PathBuf::from("/var/app/agents"),
    exe: std::env::current_exe().ok(),
    http,
});

# async fn run() -> anyhow::Result<()> {
let candidates = codync_agent::backends::launch_candidates("claude")?;
let command = candidates.into_iter().next().expect("Claude Code is known").resolve(|_| {}).await?;
let (agent, _updates) = codync_agent::acp::Acp::spawn(&command.acp(), "/path/to/project", &[])?;
let init = agent.request("initialize", serde_json::json!({"protocolVersion": 1})).await?;
# let _ = init;
# Ok(())
# }
```

## Events

What an agent sends arrives on the channel `Acp::spawn` returns, as `Incoming`:

- **`Incoming::Update`**: a `session/update` as a typed `events::Event` with the session it is about: reply text,
  reasoning, tool calls and their changes (`ToolCall`, only what was sent is `Some`; with Claude's adapter also the
  subagent call that launched it, `parent`, and on a subagent's launch what it reported, `subagent`), the plan, context usage,
  compactions, modes, config options and `/` commands. A kind this crate doesn't know yet is `Event::Other` with the
  update as sent, so a newer adapter never breaks reading.
- **`Incoming::Request`**: a request to the app, such as a permission (`Request::permission`, with the options it offers
  and `Permission::allow`). It is answered once (`respond`, `refuse`, `choose`); dropped unanswered, it answers by
  itself (a permission `cancelled`, a question `cancel`, anything else not supported), so an agent never waits on a
  request no one holds. It holds no agent alive.

Each event is a fact in the agent's own terms: defaults, filters and where it shows are the app's.

```rust,no_run
use codync_agent::acp::{Acp, Incoming, StopReason};
use codync_agent::events::Event;
use serde_json::json;

# async fn run(agent: &Acp, mut updates: tokio::sync::mpsc::UnboundedReceiver<Incoming>) -> anyhow::Result<()> {
let session = agent.session("the session id");
let turn = session.prompt(json!([{"type": "text", "text": "Fix the flaky test"}])).await?;
tokio::spawn(async move {
    while let Some(incoming) = updates.recv().await {
        match incoming {
            Incoming::Update(update) => match update.event {
                Event::Text { text, .. } => print!("{text}"),
                Event::ToolStarted(call) => println!("[{}]", call.title.unwrap_or_default()),
                _ => {}
            },
            // Allowed once, where the agent offers it; any other request refuses itself as it drops.
            Incoming::Request(request) => {
                if let Some(allow) = request.permission().and_then(|p| p.allow().map(str::to_owned)) {
                    let _ = request.choose(Some(&allow)).await;
                }
            }
            _ => {}
        }
    }
});
let ended = StopReason::of(&turn.response().await?);
# let _ = ended;
# Ok(())
# }
```

## Accounts

More than one login to the same coding agent on one computer, so several agents can run at once on different
accounts. It is the part of [Codync](https://github.com/leepokai/Codync)'s login profiles (the account kind) that is true of the agents
themselves, with no app in it.

An account is a folder (its slot) the agent keeps its login in, and the agent's own variables point a process at it.
The login the computer already has stays where it is. The agent's own CLI signs in and out: this crate never reads a
token to use it, never stores one, and never asks a provider who an account is.

| Agent | Points at the slot | Signs in | Shared with the user's setup |
|---|---|---|---|
| Claude Code (`claude`) | `CLAUDE_CONFIG_DIR` | its CLI | skills, agents, commands, plugins, rules, sessions, `CLAUDE.md`, settings without the ones that choose a login, the user's MCP servers |
| Codex (`codex`) | `CODEX_HOME` | its CLI | everything in `~/.codex` but the login files |
| OpenCode (`opencode`) | `XDG_DATA_HOME`, `XDG_STATE_HOME` | its CLI | its setup is in `~/.config/opencode`, which every account reads |
| Devin (`devin`) | `XDG_DATA_HOME`, `XDG_STATE_HOME` | its CLI | the same |
| Google Antigravity (`antigravity`) | `GEMINI_HOME`, `AGY_ACP_FORCE_FILE_STORAGE=1` | its ACP server (`authenticate`) | everything in `~/.gemini` but the server's sign-in; its conversations |
| Pi (`pi`) | `PI_CODING_AGENT_DIR` | its CLI | everything in `~/.pi/agent` but `auth.json` |
| Gemini CLI (`gemini`) | `GEMINI_CLI_HOME`, `GEMINI_FORCE_ENCRYPTED_FILE_STORAGE=false` | its CLI | everything in `~/.gemini` but its login files and `antigravity-acp` |

```rust
use codync_agent::accounts::{Account, agent};
use std::path::Path;

let claude = agent("claude").expect("Claude Code can have accounts");
let account = Account::new("0b7c".into(), claude.id, &[]);
let slot = account.slot(Path::new("/var/app/agent-accounts"))?;

// Before each start: the slot gets the user's skills, rules, MCP servers and sessions, never the login.
claude.share(Path::new("/home/me"), &slot)?;

// The agent's process, and its CLI's sign-in, run with these.
let env = claude.env(&slot, &account.id);
# Ok::<(), codync_agent::accounts::Error>(())
```

What stays with the app: where the `Account` records are saved, running the agent's sign-in (`Agent::sign_in`) and
its CLI's sign-in check (`Agent::signed_in`; `Agent::signed_in_on_disk` answers for a login kept in a file), and refusing to remove an account something still runs on. After a sign-in,
`Agent::identity` reads who the slot is signed in as from the agent's own files.

Folders are linked (a junction on Windows, which needs no privilege), so both sides see one copy. Files an agent
rewrites by rename are copied before each start, since a rename replaces a link.
