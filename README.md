# Codync

**Your coding agents, as teammates you can message.**

Codync turns Claude Code, Codex, OpenCode, Grok Build and Gemini CLI into persistent *bots* you delegate to from your iPhone — the way you'd message a colleague. Pick who, say what, put the phone away. You get a notification when a bot finishes or needs your approval.

> Why bots? On a phone, "find the right working session, then pick an environment" is too slow. With bots you already know who to hand the intent to: open the chat, type, done.

[![Download on the App Store](https://img.shields.io/badge/App_Store-iOS-blue?logo=apple)](https://apps.apple.com/tw/app/codync/id6760984418?l=en-GB)
[![Homebrew](https://img.shields.io/badge/Homebrew-codync-orange?logo=homebrew)](https://github.com/leepokai/homebrew-codync)

## How it works

```
iPhone (Codync)  ⇄  HTTP + SSE over Tailscale / Wi-Fi  ⇄  codync-host  ⇄  ACP (stdio)  ⇄  claude · codex · opencode · grok · gemini
       ▲                                                     │
       └──────── APNs ◀── relay (Cloudflare Worker) ◀────────┘  "needs you" / "done"
```

- **Bots** have a name, a character avatar, standing instructions, an agent backend, a project folder and a permission policy. Each bot is **one endless conversation**; the agent sessions underneath are an implementation detail (resumed with `session/load`, restarted with *New session*).
- **The chat only shows what matters**: your messages, each turn's final reply, approval cards and notices. Every tool call, diff, plan and thought is one tap away in *Full conversation*. While a bot works, its row shows what it's doing right now.
- **codync-host** (Rust, macOS + Linux) runs on your computer. It speaks the [Agent Client Protocol](https://agentclientprotocol.com) to each agent, keeps transcripts in SQLite, and serves the phone. Every change carries a global `rev`, so the phone reconnects with `since: rev` and never misses anything.
- **Usage limits**: the host reads Claude (statusline `rate_limits`, or the OAuth usage endpoint with the token Claude Code already stored) and Codex (`~/.codex/sessions`) limits. Tokens never leave your computer; the phone and widgets only see percentages.
- **Push** goes through a tiny relay that holds the APNs key. The phone trades its device token for an encrypted ticket; the host only ever holds tickets.

UI patterns (roster, character avatars, approval cards, trace sheet, "needs you / done" notifications) follow Grok Bot.

## Install

**Mac**

```bash
brew install --cask leepokai/codync/codync
```

Open Codync in the menu bar → **Install host** → **Pair iPhone**, and scan the code with the Codync app.

**Linux**

```bash
brew install leepokai/codync/codync-host   # or download a release tarball
codync-host install                        # systemd --user service
codync-host pair                           # shows the QR code in the terminal
```

Install [Tailscale](https://tailscale.com) on the computer and the phone to reach your bots from anywhere.

**Agents** — install and log in to whichever you use: `claude`, `codex`, `opencode`, `grok`, `gemini`. Claude Code and Codex are driven through their ACP adapters (`@agentclientprotocol/claude-agent-acp`, `@agentclientprotocol/codex-acp`), which `npx` fetches on first use, so Node.js is required for those two.

Optional: show usage in Claude Code's status line and feed live limits to the host:

```json
{ "statusLine": { "type": "command", "command": "codync-host statusline" } }
```

## `codync-host`

| Command | |
|---|---|
| `codync-host install` / `uninstall` | background service (launchd on macOS, systemd `--user` on Linux); also removes Codync 1.x Claude hooks |
| `codync-host pair [--json]` | pairing QR code / link |
| `codync-host status` | installed? running? |
| `codync-host serve [--port 19222]` | run in the foreground |
| `codync-host statusline` | Claude Code status line command |
| `codync-host reset-token` | unpair every phone |

Data lives in `~/.codync` (`codync.db`, `token`, `host.log`). The API is `POST /api/<method>` + `GET /events` (SSE), both with `Authorization: Bearer <token>`; see `host/src/api.rs`.

## Repository

| Path | |
|---|---|
| `host/` | `codync-host` — Rust daemon: ACP client, SQLite transcript, HTTP/SSE API, push, usage |
| `Codync-iOS/` | iOS app: roster, threads, approval cards, trace, bot editor, pairing |
| `CodyncWidgets/` | Usage widget + bot Live Activity |
| `Codync-macOS/` | Menu bar app: installs/monitors the host, pairing QR, usage |
| `CodyncKit/` | Shared Swift package: wire models, host client, theme, avatars |
| `relay/` | Cloudflare Worker APNs relay with encrypted per-device tickets |
| `packaging/` | Homebrew formula template |

The Xcode project is generated: `xcodegen generate`.

```bash
cd host && cargo test            # host
cd CodyncKit && swift test       # shared Swift
cd relay && npm test             # relay tickets
```

## Versioning

The major version is the phone ↔ host protocol: 2.x apps work with 2.x hosts. Codync 2 replaces the 1.x hook/CloudKit monitor entirely.

## License

MIT
