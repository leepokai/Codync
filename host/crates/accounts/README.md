# codync-accounts

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
use codync_accounts::{Account, agent};
use std::path::Path;

let claude = agent("claude").expect("Claude Code can have accounts");
let account = Account::new("0b7c".into(), claude.id, &[]);
let slot = account.slot(Path::new("/var/app/agent-accounts"))?;

// Before each start: the slot gets the user's skills, rules, MCP servers and sessions, never the login.
claude.share(Path::new("/home/me"), &slot)?;

// The agent's process, and its CLI's sign-in, run with these.
let env = claude.env(&slot, &account.id);
# Ok::<(), codync_accounts::Error>(())
```

What stays with the app: where the `Account` records are saved, running the agent's sign-in (`Agent::sign_in`) and
its CLI's sign-in check (`Agent::signed_in`; `Agent::signed_in_on_disk` answers for a login kept in a file), and refusing to remove an account something still runs on. After a sign-in,
`Agent::identity` reads who the slot is signed in as from the agent's own files.

Folders are linked (a junction on Windows, which needs no privilege), so both sides see one copy. Files an agent
rewrites by rename are copied before each start, since a rename replaces a link.
