# Codync

**Your coding agents, as teammates you can message.**

Codync turns the coding agents on your computer — Claude Code, Codex, Cursor, Pi, OpenCode, Grok Build, Gemini, Copilot and ~40 more — into persistent *bots* you delegate to from your iPhone, Mac, Linux desktop or any terminal, the way you'd message a colleague. Pick who, say what, put the phone away. You get a notification when a bot finishes or needs your approval.

> Why bots? On a phone, "find the right working session, then pick an environment" is too slow. With bots you already know who to hand the intent to: open the chat, type, done.

[![Download on the App Store](https://img.shields.io/badge/App_Store-iOS-blue?logo=apple)](https://apps.apple.com/tw/app/codync/id6760984418?l=en-GB)
[![Homebrew](https://img.shields.io/badge/Homebrew-codync-orange?logo=homebrew)](https://github.com/leepokai/homebrew-codync)

## How it works

```text
Remote Apple client ⇄ encrypted channel (direct or Cloudflare cloud/) ⇄ codync-host ⇄ ACP agent
Local / SSH client  ⇄ loopback HTTP + SSE                             ⇄ codync-host
Host notifications → APNs worker (relay/) → iPhone
```

- **Bots** have a name, a character avatar, standing instructions, an agent backend, a project folder and a permission policy. Each bot has **a persistent main chat and optional reply threads**; the agent sessions underneath are an implementation detail (resumed with `session/load`, restarted with *New session*).
- **Bots can ask each other for help.** Tell one “ask Reviewer to check these changes.” The built-in `team` MCP server lets it discover your other visible bots and wait for a reply. Requests appear in both chats; each recipient keeps its own agent, folder and approvals. Native subagents stay under the coding agent's control. See [bot collaboration](docs/features/bot-collaboration.md).
- **The chat only shows what matters**: your messages, each turn's final reply, approval cards and notices. Every tool call, diff, plan and thought is one tap away in *Full conversation*. While a bot works, its row shows what it's doing right now.
- **Agents are detected automatically**: the host hydrates PATH from your login shell (plus Homebrew, ~/.local/bin, nvm, bun, volta, asdf, mise, pnpm…), finds every harness you have installed, and prefers its native ACP mode. Anything else in the official [ACP registry](https://agentclientprotocol.com/registry) can be picked too — Codync fetches it on first use (npx, uvx or a checksummed binary).
- **codync-host** (Rust, macOS + Linux) runs on your computer. It speaks the [Agent Client Protocol](https://agentclientprotocol.com) to each agent, keeps transcripts in SQLite, and serves the phone. Every change carries a global `rev`, so the phone reconnects with `since: rev` and never misses anything.
- **Usage limits** come from your local installs, with no extra login: Claude via `claude -p /usage` (a local command, no model call) plus Claude Code's status line, Codex via `~/.codex/sessions`. The phone, widgets and menu bar only see percentages.
- **Push** goes through a tiny relay that holds the APNs key. The phone trades its device token for an encrypted ticket; the host only ever holds tickets.

UI patterns (roster, character avatars, approval cards, trace sheet, "needs you / done" notifications) follow Grok Bot.

## Install

**Mac** — [download codync-macos.dmg](https://github.com/leepokai/Codync/releases/latest/download/codync-macos.dmg) and drag it to Applications, or:

```bash
brew install --cask leepokai/codync/codync
```

Both install the same signed, notarized app; the host ships inside it.

Open Codync in the menu bar → **Install host**. The chat bubble opens the full Codync window; the QR button pairs your iPhone.

**Linux** — native GTK 4 / libadwaita app plus the host:

```bash
brew install leepokai/codync/codync-host   # or a release tarball
codync-host install                        # systemd --user service
codync                                     # the desktop app (or codync-linux-x86_64.tar.gz / -arm64 from Releases)
codync-host pair                           # QR code in the terminal, or Settings in the app
```

Building the Linux app yourself needs `libgtk-4-dev libadwaita-1-dev`: `cargo install --path apps/linux`.

Remote access uses an encrypted channel over direct LAN/Tailscale or the Cloudflare relay. Tailscale is optional. Configure matching app/host environments and follow the [Cloudflare test guide](docs/guides/cloudflare-testing.md); development configuration exists, while production configuration still needs completion.

**Agents** — install and sign in to whichever you use; Codync finds them. Claude Code, Codex and Pi run through their ACP adapters (fetched by `npx`, so Node.js is needed for those).

`codync-host install` also routes Claude Code's status line through `codync-host statusline` so live limits reach the host; an existing status line keeps working (it's wrapped, and restored on `uninstall`).

## `codync-host`

| Command | |
|---|---|
| `codync-host install` / `uninstall` | background service (launchd on macOS, systemd `--user` on Linux) |
| `codync-host tui [--url … --token …]` | message your bots from a terminal (this computer by default; SSH-friendly) |
| `codync-host pair [--json]` | pairing QR code / link |
| `codync-host status` | installed? running? |
| `codync-host serve [--port 19222]` | run in the foreground |
| `codync-host statusline [-- <your command>]` | Claude Code status line command (wraps yours) |
| `codync-host reset-token` | rotate the local loopback bearer token |
| `codync-host cloud` | cloud enablement, URL and status |
| `codync-host devices` | list/revoke authorized remote devices |
| `codync-host access` | review device access requests |

Data lives in `~/.codync`. The local bearer token authorizes loopback helpers and SSH-forwarded callers. Remote devices use individual keys and grants over the encrypted channel; revoke a lost device through device management rather than rotating the loopback token. Host methods and caller permissions live in `host/src/api.rs`.

## Repository

| Path | |
|---|---|
| `host/` | `codync-host` — Rust daemon: ACP client, SQLite transcript, HTTP/SSE API, push, usage |
| `kit/` | Swift package: `CodyncKit` (wire models, host client, theme, avatars) and `CodyncUI` (store + chat screens shared by iPhone and Mac) |
| `apps/ios/` | iOS app: pairing, roster, push, Live Activity glue |
| `apps/ios/Widgets/` | Bots, usage and per-provider usage widgets + bot Live Activity |
| `apps/macos/` | Menu bar + native chat window; installs/monitors the host |
| `apps/linux/` | Native Linux app (GTK 4 + libadwaita, Rust) |
| `cloud/` | Cloudflare accounts, D1, encrypted channel relay and offline mailbox |
| `apps/shared/` | Shared Apple account integration and environment configuration |
| `apps/screen-macos/`, `apps/screen-linux/` | Platform screen capture/input helpers |
| `docs/` | [Documentation index](docs/README.md) and [file structure](docs/architecture/file-structure.md) |
| `relay/` | Cloudflare Worker APNs relay with encrypted per-device tickets |
| `web/` | Website (git submodule) |
| `packaging/` | Homebrew formula template |

Build, install, restart and full checks: [development guide](docs/guides/development.md).

The Xcode project is generated: `xcodegen generate`.

```bash
cd host && cargo test            # host
cd kit && swift test             # shared Swift
cd apps/linux && cargo test      # Linux app (needs GTK dev packages)
cd cloud && npm test            # cloud API / channel relay
cd relay && npm test             # relay tickets
```

## Versioning

The major version is the phone ↔ host protocol: apps work with hosts of the same major version.

## License

MIT
