# Usage limits

What the apps show about plan limits, read on the host from the agents on the computer. Each agent's reader is in the `codync-agent` crate (`host/crates/agent/src/usage/`, its `usage` feature); which agents are read, when, and what the apps get is the host's (`host/src/usage/`). A reading never starts a model turn, and the host calls no provider API with an agent's login (Cursor's dashboard is the one exception, see [Cursor usage](marketplace.md#cursor-usage)).

## Which agents show

Every Usage screen can list the agents that can report on this computer, each with a switch, behind the button beside the computer's name: the desktop's **Usage** page, the phones' **State > Usage limits**, and the terminal UI's `U` then `s`. The choice is per computer and lives on its host, so every app, widget, the menu bar and the notch island follow it.

- **Default:** an agent shows when a bot on this computer runs it, as its own agent or a fallback. Being installed or signed in isn't enough: a Cursor app that's signed in stays off until a bot uses Cursor or the user turns it on.
- **A choice wins over the default:** `setUsageShown {id, shown}` saves `usage.shown.<id>` in the host's kv and answers with the new `usage`.
- **One order per computer:** `setUsageOrder {ids}` saves `usage.order`, and the host sorts `services` and `providers` by it, so every Usage screen, the menu bar and the terminal UI follow. The notch island's list and the widgets' headline stay tightest first, and the widgets' other rows follow the order. An agent the order doesn't name follows the others in `READERS` order. The switches and the order live behind one button on each Usage screen; in the terminal, `J` and `K` move a row in the chooser.
- **Reading follows the list:** an agent nobody runs or asked for is never started, and its saved login is never read. One that becomes wanted (a new bot's, one just turned on) is read right away instead of at the next five-minute poll, through `Hub::usage_wake`.
- **Hidden isn't unread:** an agent a bot still runs keeps being read while hidden, because work (`near_limit`) and fallbacks (`rest::reset_time`) go by its limits. `hub.usage` holds every reading; the apps get `Usage::visible()`.
- **Listed:** wanted, holding a reading, its CLI installed (`backends::installed`), or something to read without the CLI (`Reader::found_without_cli`: Cursor's saved login, a JetBrains IDE's quota file).

On the wire, `usage` gains `services: [{id, name, shown, registry?}]` next to `providers`, which leaves out the hidden agents. A host older than the chooser sends no `services`, and the apps then show no switches. A shown agent with no reading says **No reading yet**: signed out, or nothing to report. Providers and services carry the agent's ACP `registry` id when it has one, and clients draw its logo by `registry ?? id`.

### A check that fails

A check that fails (a reader's `None`, a login profile's `Reading::Unknown`) keeps the login's windows and its `updatedAt`, and sets `failedAt`, the first failure since the last reading, on the provider (the default login) or its `loginProfiles` row (`Usage::land`, `Checked`). A probe doesn't mark default-login windows another source wrote (Claude's rate-limit event or status line, which a `/usage` with no plan limits can't read). The next reading clears it; a check that keeps failing changes nothing, so it sends no event. A failed check counts as asked, so it isn't read again before the poll (`services::unread_readers`). Every Usage screen (phones, desktop Settings, the terminal UI) puts a quiet **Out of date** under a login with windows and a `failedAt`; one with no windows still says **No reading yet**. The widgets, the menu bar and the notch island don't mark it. An older app ignores the field. A reading skipped because the same agent is already being read lands nothing: that reading lands instead (`codync_agent::usage::busy`); so does one Copilot's reader skips on purpose between its reads, every 30 minutes (`Outcome::Skipped`). A status-line or rate-limit reading, which brings only some windows, clears the mark too, and on a marked login drops the windows it didn't bring: they date from before the failure and nothing would mark them any more (`Checked::land`).

`usage` with `refresh: true` answers within a minute with what has landed by then; a slower reader (a first download) carries on and lands through `usage` events. Every change goes through `Hub::update_usage`, under the lock, so a reading landing during a sync isn't written over.

Adding an agent is its reader in the crate (`codync_agent::usage::<agent>`) and one row in `READERS` (`host/src/usage/services.rs`). The reader's `id` is the harness id, so a bot's `backend` finds it (`backends::harness_of`, which also answers to the registry id).

## Login profiles

An agent with signed-in [account login profiles](login-profiles.md) lists every login under its name, the default one
first, each with its own windows. An agent on one login looks as it always did.

- **On the wire:** a provider's `windows` stay the default login's. `loginProfiles: [{id, name, windows, updatedAt, failedAt?}]`
  holds the accounts, and `defaultLoginProfile` what the default login is called beside them (the user's name for it,
  or who it signs in). Both are absent while the agent has one login, so an older app shows the default login as before.
- **Which rows:** `services::sync` lists the signed-in accounts of each agent that gets read
  (`login_profiles::usage_rows`). An API key has no plan, and a signed-out account nothing to ask. A profile
  renamed, removed, signed in or signed out (a turn refused for auth too) changes its row right away
  (`usage::login_profiles_changed`), and a new row is read without waiting for the poll; so is a default login that
  has no reading yet. The default logins' names follow each sign-in refresh.
- **How they're read:** the same local reading as the default login, with the agent's own variable pointing its CLI at
  the profile's folder (`Reader::read_profile`): `claude -p /usage` under `CLAUDE_CONFIG_DIR`, `codex app-server` under
  `CODEX_HOME`. The profile can't be removed while that check runs (`login_profiles::hold`). Claude's status line and rate-limit events land on the row of the login their session runs on (the
  status line hook sends `CODYNC_AGENT_ACCOUNT` as `?loginProfile=`). No provider API is called. The other agents'
  accounts list with **No reading yet**.
- **A row with nothing read** says **No reading yet**; so does the default login when only an account has numbers,
  and the agent's row in the chooser while none of its logins has any.
- **What goes by them:** work (`near_limit`) and a fallback's rest (`rest::reset_time`) go by the windows
  of the login the bot runs on (`Usage::windows`).
- **One agent, one line:** the notch island, the widgets and the mascot's "nearly used up" show the login nearest a
  limit (`tightestLogin`), without its name. The Usage screens and the terminal UI's sheet show all.
- **The menu bar, a level at a time** (`shared/tray-usage.ts`): an agent on one login lists its limits under its name,
  and one with several is a single row that opens every login's, a login with nothing read saying **No reading yet**.
  With more than one computer in the app, a **Usage limits** title leads a row per connected computer that has limits
  to show (a paused or offline one is left out rather than shown with old numbers), and each opens its own agents.

## Readers

| Agent | How it reads | Checked against |
|---|---|---|
| Claude | `claude -p /usage`, the status line, ACP rate-limit events | a real login |
| Codex | the CLI's `account/rateLimits/read` ([Codex usage](marketplace.md#codex-usage)) | a real login |
| Google Antigravity | its ACP server's `/usage`, or the running app's language server | a real login (ACP) |
| Cursor | the saved login, then its dashboard service ([Cursor usage](marketplace.md#cursor-usage)) | a real login |
| GitHub Copilot | `account.getQuota` on `copilot --headless --stdio` | a real login |
| Hermes Agent | `hermes usage --json` | source only: this provider had no account reading |
| Grok Build | the `x.ai/billing` ACP extension | the 1.0.75 binary's strings; no login |
| Auggie | `auggie account status --json` | the 0.36.0 source; no login |
| Kilo | `kilo profile --json`, the dollar balance as a note | the 7.8.18 source; no login |
| Amp, Junie, Kimi Code, Kiro CLI, Mistral Vibe, oh-my-pi, OpenCode | their own commands, logs or files (each reader's header) | as each says |

The readers checked only against source fail closed: anything unexpected is no reading, never a wrong number. Check one against a real login when a computer has it.

### Claude's rate-limit event

The Claude adapter passes the Agent SDK's `rate_limit_info` through whole, in `usage_update._meta["_claude/rateLimit"]`. Its own `utilization` comes only with a warning or a refusal. A limit that is merely `allowed` has it in `unifiedWindows` (`five_hour`, `seven_day`, `seven_day_overage_included`), which the host reads too; the limit the event is about wins over its entry there. Claude Code marks that field internal and the SDK's types leave it out, so it can go away: an event without it reads as before. `seven_day_overage_included` reads into the `/usage` report's **Weekly · Fable** row (`seven_day_fable`): Claude Code's own name for that kind is "Fable limit", and the event names no model, so the name is fixed in the reader.

On a computer where Claude runs on a `claude setup-token` token (`CLAUDE_CODE_OAUTH_TOKEN`), this is the only reading: that token only runs models, so `claude -p /usage` reports no plan limits. The limits then show after a bot's first turn, and only the windows the event lists.

`claude -p /usage` itself always asks for the subscription's limits: Claude Code routed off it (a gateway's `ANTHROPIC_BASE_URL` with `ANTHROPIC_AUTH_TOKEN` or `ANTHROPIC_API_KEY`, or `CLAUDE_CODE_USE_BEDROCK` / `VERTEX` / `FOUNDRY`, in the environment or in its settings' `env`) answers it with token costs and no limits ([#372](https://github.com/leepokai/Codync/issues/372)). So the check passes `--settings` with those six variables blank (`host/crates/agent/src/usage/claude.rs`), which overrides both the environment and the user, project and local settings; the rest of the settings' `env`, a proxy for one, still applies, and a login profile keeps its `CLAUDE_CONFIG_DIR`. When the check still fails, the login keeps its last windows and time and is marked **Out of date** (see [A check that fails](#a-check-that-fails)).

### An agent's own slash command

`host/crates/agent/src/usage/acp_read.rs` runs an ACP slash command that the adapter answers itself. Antigravity's `/usage` is one: its ACP server handles it with no LLM and asks Google with its own login, so the host never touches that login.

- **Only when advertised.** The command is sent only after the session lists it in `available_commands_update`. Sent to an agent that doesn't know it, it would reach the model and spend the plan it was meant to read.
- **One session per agent,** resumed for every reading (`usage.session.<id>` in the host's kv): each new Antigravity session leaves a conversation database behind, about 400 KB.
- **Never a browser.** A signed-out Antigravity starts its browser sign-in as soon as a session opens, and it would do so at every poll. The reading runs with `BROWSER=true`, which its Python honors, then gives up after 30 seconds. Windows has no such command, so there it needs a saved login file first.
- **One install at a time.** A reading and a sign-in check can start the same registry agent at once: its installs run one at a time and check again once they get their turn.
- **A download isn't cut short.** No timeout wraps a reader's start, or a slow connection would start over at every poll. The handshake has its own.

The report is Markdown meant for people, so an update can change it; `parse` is how that surfaces.

### Failures

`usage_read_failed` reports once per failing run, with the stage, for the readers that start the agent's server (Antigravity, Grok Build, Copilot) or read a CLI's JSON report (Hermes, Auggie, Kilo): a format change shows up as `parse`, which is the only way to learn of one in a reader never run against a login. Nothing printed, or a CLI's words for being signed out, isn't a failure. A document that says there's nothing to read is an empty reading, which removes the row, as Kilo's balance of 0 does: the CLI prints that when its own balance request fails, so it's neither a claim that the account is empty nor a reason to keep an old balance.

### Copilot's log files

Each start of Copilot's headless server leaves an empty file in `~/.copilot/logs`, even with `--log-level none`, so it's read every 30 minutes rather than every five, and 10 minutes after a failure, such as being signed out. Turned off and on within that time, it gets its last reading back without a new start.

## Agents not read

Only what's been looked at, as of 2026-11-06. None of these were run against a login.

| Agent | Why not |
|---|---|
| Factory Droid, MiniMax Code, Gemini CLI | Plan limits exist, but only in the TUI. A PTY reader would type into it, where a command it doesn't know goes to the model. Never `gemini -p "/stats"`: that's a real turn. |
| Qoder | `get_usage_info` is in a newer SDK than the registry's CLI, and starting it outside the SDK's daemon is unverified. |
| Cline, Devin | Plan numbers only through their HTTP APIs. |
| CodeBuddy Code, WorkBuddy, Cortex Code | Nothing found outside their websites. Their bundles weren't searched in full. |
| Qwen Code, Poolside, Pi, DeepSeek Harness, goose | Session tokens only, or bring-your-own-key with no plan. |
| Doubao | No usage command in `doubao --help`. Plan numbers weren't searched beyond that. |
