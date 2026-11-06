# Dev servers

A bot can leave a program running on the computer: a dev server, a watch build. The host runs it, not the agent, so it
keeps running when the agent stops, crashes or restarts, and every client shows its port and can stop it. It came out
of [#343](https://github.com/leepokai/Codync/issues/343), where an agent kept alive for its dev server also kept its
MCP helpers.

It is independent of the bot, not isolated: a dev server runs as the user, with the user's files and network. The
code is in `host/src/dev_servers/`.

## Why the host runs them

A program started through an agent's shell tool is a descendant of the agent. On Windows the agent's job ends
everything in it. On macOS and Linux the host kills the agent's process group, so a program in that group goes too, and
one that detached into its own session survives with nobody tracking it. Taking the tree down with the agent is
deliberate, and stays right for what an agent starts itself. So that such a server isn't lost, `background_work` keeps
the agent running while it sees one, with its helpers and memory.

A server started through `start_dev_server` is the host's own child, in a tree of its own (`agent/process.rs`, the same
owner agents use), so the agent can be stopped when idle while it runs.

| Event | The server |
|---|---|
| Agent stopped when idle, crashed or changed; new session; routine run ended | Keeps running |
| User or bot stops it | Its whole tree stops: SIGTERM, then SIGKILL after 3 s on Unix; the job ends at once on Windows |
| It ends by itself | Stopped (exit 0) or failed; it doesn't start again |
| Bot deleted | Stopped and removed |
| Host updated or restarted | Stopped, then started again |
| Computer restarted | Listed as stopped; one action starts it. An abandoned server shouldn't come back at every login |
| Host crashed | Windows: the job closes and it ends. Unix: it survives, and the next host start kills it, matching pid and process start time, so it can't hold the port |

`ponytail:` it doesn't survive the host, so its in-memory state is lost on a host update. Surviving would take log files
instead of pipes, a named job on Windows and re-adopting by pid.

## Bot tools

On the existing `chat` helper (`dev_servers/tools.rs`): a new MCP server would add a process to every session.

| Tool | What it does |
|---|---|
| `start_dev_server {command, name?, folder?}` | Starts it, in the bot's folder by default. The same bot, folder (`.` and a trailing separator don't count) and command returns that server; a stopped one starts again, without a new card if someone allowed it on one. Waits up to 20 s for a port or an exit, then answers with the status, ports and the end of its log |
| `list_dev_servers` | This bot's servers, what it left running through its shell included |
| `stop_dev_server {id}` | Stops one of this bot's servers |
| `dev_server_logs {id, lines?}` | The end of its log (200 lines by default, at most 1000) |

- At most 5 run per bot, however they start (a bot or a person); past that the error lists them.
- A start finishes in a task of its own, so a request that goes away (a closed connection, a cancelled tool call) can't leave it half started. A stop that comes meanwhile ends it as soon as it's up.
- The command ends when its first process ends, so bots give it as they'd run it in the foreground.
- Bots hear about them in the tool's description and one line of their instructions (`BRIEF`). Nothing forces it: see
  [what a bot leaves running](#what-a-bot-leaves-running).
- Output goes to `~/.codync/dev-servers/<id>.log`, the current run only, rotated at 5 MB.
- The environment is the host's own, never the agent's, which can hold its API keys.

## Approval and safety

- **Under Ask, a start waits on an approval card** with the command and folder (`chat::manage::approve`, as `update_bot`).
  Under Auto it runs, like the agent's own commands. A server first started under Auto had no card, so it gets one when
  the bot, now under Ask, starts it again (`Saved::approved`).
- **The command runs outside any sandbox the agent has** (Codex has one). A sandboxed agent under Auto therefore has a
  way out of it through this tool. The card under Ask is the control; decided 2026-11-05 to match the agent's own
  commands under Auto.
- **Clients never send a command.** They name a server (`devServerStop`, `devServerRestart`, `devServerRemove`,
  `devServerLogs`), so a phone can only re-run what a bot started. The bot tools stay loopback-only (`chatCall` is in
  `LOCAL_ONLY`), and a bot reaches only its own servers.
- A command can hold a secret, so the tool's description says to keep them in the project's env files. Commands reach
  clients over the E2E channel, and never reach analytics.

## Ports

No guessing what counts as a dev server: **one that listens gets its port shown**, one that doesn't (a watch build) shows
only its status.

- `dev_servers/ports.rs` reads TCP listeners with the [`listeners`](https://crates.io/crates/listeners) crate and keeps
  those in the server's tree (`Process::members`: its process group on Unix, its job on Windows).
- `dev_servers/scan.rs` reads them every second for a server's first 30 s, then every 10 s, and tells clients only of
  changes. A server with no port after 5 s counts as running.
- The host works out the addresses, so clients don't: `local` works on the computer, `remote` from other devices. A
  loopback bind has no `remote`; a bind on all interfaces gets the computer's Tailscale and LAN addresses, or its public
  one on a machine with neither, because that's where it is open. They're worked out again when the computer's addresses
  change (a scan compares `if_addrs`, which starts no process; `tailscale status` does).

## Wire

Additive, so no `minApp` raise: non-group bots carry `devServers`.

```json
"devServers": [{
  "id": "…", "name": "web", "command": "npm run dev", "folder": "/Users/me/site",
  "status": "running",
  "ports": [{"port": 5173, "local": "http://localhost:5173", "remote": ["http://192.168.1.20:5173"]}],
  "startedAt": 1793923200000, "exitCode": null, "managed": true
}]
```

`status` is `starting`, `running`, `stopped` or `failed`. `managed` is false for what a bot left running through its shell.

## UI

Nothing new in the chat beyond the port itself, and no color but the danger color for a failure: the apps are black and
white, and color comes from the bots.

- **Title:** the desktop's title pill shows `Builder :5173` in quiet text; the phones' subtitle shows `Connected · :5173`;
  the terminal's header shows it on the right. With more running it's `:5173 +2`; with no port, the server's name; none
  running, `Starting` or `Failed to start`; stopped, nothing. Opening the bot's details is what the title already does.
- **Details:** a **Dev servers** section first on the Details tab, shown only when the bot has servers. A row is one line:
  name and port (or `Running`, `Starting`, `Stopped`, `Failed to start`). A click opens it in the browser, or with no port
  to open (a watch build, or one that ended) its logs; the rest (copy address, logs, restart, stop, remove) is behind
  `⋯`, which on the desktop shows only when the row is hovered.
- **Other devices:** a server listening only on its computer can't be opened from a phone, or from a desktop over SSH
  (`store === app.local`, not `isLoopback`: an SSH store is loopback through its tunnel). Its menu says so. A server on all
  interfaces opens at its first `remote` address, which this device may not reach (a Tailscale address, without Tailscale
  here), so with several the menu lists each one to open, in place of `Open`.
- **Logs:** a sheet with the last 200 lines and a refresh button; for a failed server, its exit code.
- **Terminal:** `/servers` or a click on the port opens a sheet instead of a section; keys in `tui/manage/dev_servers.rs`,
  the footer listing those that apply to the selected server. `codync-host tui --url` may be a tunnel, which looks local,
  so its sheet lists the selected server's addresses, one line each, instead of opening a browser.

Each client: desktop `views/thread/DevServersSection.tsx` and `src/shared/dev-servers.ts`; iPhone
`CodyncUI/Bots/DevServersSection.swift`; Android `DevServersSection.kt`; terminal `tui/manage/dev_servers.rs` and
`tui/view/dev_servers.rs`.

## What a bot leaves running

Instructions aren't a guarantee, and some agents have their own background-shell feature. `dev_servers/detected.rs` lists
what an agent started outside Codync's helpers and listens below port 32768 (above is where systems hand out ports: an
agent's own callback or inspector). The agent's own processes, by program name (its harness's `bins` and its command's
first word, unless that's a runtime like `node`), don't count: `opencode acp` listens on 4096. Processes that share a
socket are one server (a pre-fork server), whose Stop ends them all. A Stop reads the agent's tree again and ends only what
is still in it, since a pid from the last scan can be reused; when the tree can't be read it fails and the row stays. These
show with `managed: false`, marked as stopping with the bot, and offer only open, copy and stop: they can't leave the
agent's tree. `background_work` still keeps their agent running. The `dev_server_detected` event measures how often this
happens.

## Tests

`host/tests/dev_server_e2e.rs` runs on every system host CI does, and walks the table above: its Unix-only parts are a
crashed host's leftover process, an agent stopped after a routine's run (Windows can't tell that fixture from one with
a server, see `background_work`) and a process list that can't be read. `apps/desktop/e2e/dev-servers.spec.ts` and
`apps/maestro/dev-servers.yaml` walk the clients.

## Later

- **Phone preview:** tunnel a port over the E2E channel into an in-app web view. Hot reload needs WebSockets, so it's a
  plan of its own. A cheaper step first: open the page on the computer and watch it over remote screen.
- **Desktop over SSH:** forward the port with the tunnel code in `apps/desktop/src/main/ssh.ts`.
- **All servers on one screen**, across bots, when one bot's details stop being enough.
- **Starting a server from the apps**, which would mean clients sending commands, so it needs its own safety design.
