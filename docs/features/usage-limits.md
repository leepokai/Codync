# Usage limits

What the apps show about plan limits, read on the host (`host/src/usage/`) from the agents on the computer. A reading never starts a model turn, and the host calls no provider API with an agent's login (Cursor's dashboard is the one exception, see [Cursor usage](marketplace.md#cursor-usage)).

## Which agents show

Every Usage screen can list the agents that can report on this computer, each with a switch, behind the button beside the computer's name: desktop **Settings > Usage**, the phones' **State > Usage limits**, and the terminal UI's `U` then `s`. The choice is per computer and lives on its host, so every app, widget, the menu bar and the notch island follow it.

- **Default:** an agent shows when a bot on this computer runs it, as its own agent or a fallback. Being installed or signed in isn't enough: a Cursor app that's signed in stays off until a bot uses Cursor or the user turns it on.
- **A choice wins over the default:** `setUsageShown {id, shown}` saves `usage.shown.<id>` in the host's kv and answers with the new `usage`.
- **One order per computer:** `setUsageOrder {ids}` saves `usage.order`, and the host sorts `services` and `providers` by it, so every Usage screen, the menu bar and the terminal UI follow. The notch island's list and the widgets' headline stay tightest first, and the widgets' other rows follow the order. An agent the order doesn't name follows the others in `READERS` order. The switches and the order live behind one button on each Usage screen; in the terminal, `J` and `K` move a row in the chooser.
- **Reading follows the list:** an agent nobody runs or asked for is never started, and its saved login is never read. One that becomes wanted (a new bot's, one just turned on) is read right away instead of at the next five-minute poll, through `Hub::usage_wake`.
- **Hidden isn't unread:** an agent a bot still runs keeps being read while hidden, because background work (`near_limit`) and fallbacks (`rest::reset_time`) go by its limits. `hub.usage` holds every reading; the apps get `Usage::visible()`.
- **Listed:** wanted, holding a reading, its CLI installed (`backends::installed`), or something to read without the CLI (`Reader::found_without_cli`: Cursor's saved login, a JetBrains IDE's quota file).

On the wire, `usage` gains `services: [{id, name, shown, registry?}]` next to `providers`, which leaves out the hidden agents. A host older than the chooser sends no `services`, and the apps then show no switches. A shown agent with no reading says **No reading yet**: signed out, or nothing to report. Providers and services carry the agent's ACP `registry` id when it has one, and clients draw its logo by `registry ?? id`.

`usage` with `refresh: true` answers within a minute with what has landed by then; a slower reader (a first download) carries on and lands through `usage` events. Every change goes through `Hub::update_usage`, under the lock, so a reading landing during a sync isn't written over.

Adding an agent is one row in `READERS` (`usage/services.rs`). The reader's `id` is the harness id, so a bot's `backend` finds it (`backends::harness_of`, which also answers to the registry id).

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

### An agent's own slash command

`usage/acp_read.rs` runs an ACP slash command that the adapter answers itself. Antigravity's `/usage` is one: its ACP server handles it with no LLM and asks Google with its own login, so the host never touches that login.

- **Only when advertised.** The command is sent only after the session lists it in `available_commands_update`. Sent to an agent that doesn't know it, it would reach the model and spend the plan it was meant to read.
- **One session per agent,** resumed for every reading (`usage.session.<id>` in the host's kv): each new Antigravity session leaves a conversation database behind, about 400 KB.
- **Never a browser.** A signed-out Antigravity starts its browser sign-in as soon as a session opens, and it would do so at every poll. The reading runs with `BROWSER=true`, which its Python honors, then gives up after 30 seconds. Windows has no such command, so there it needs a saved login file first.
- **A bot's build is left alone.** Installing a new registry build removes the one before it, so while a bot here runs that agent and its current version isn't installed, readings wait for that bot's next start to bring it (`backends::update_pending`). Installs run one at a time and check again once they get their turn.
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
