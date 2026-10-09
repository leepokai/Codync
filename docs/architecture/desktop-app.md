# Desktop app (macOS, Linux and Windows)

Codync's computer-side app is one Electron app in `apps/desktop/` for macOS, Linux and Windows. It
replaced the SwiftUI Mac app (`apps/macos/`) and the GTK 4 Linux app (`apps/linux/`) in 2.7.0.
The iPhone app stays native SwiftUI; its package moved from `kit/` into `apps/ios/Kit/`.

## Why

- **The chat was too slow.** Long transcripts with Markdown, variable-height rows and
  streaming replies are where SwiftUI's lazy stacks struggle: scrolling and following a reply
  as it streams needed an AppKit scroll view with hand-managed row hosting to stay usable. A
  browser engine lays out and scrolls long text natively.
- **Two desktop apps were too much to keep in step.** Every UI change had to land in SwiftUI
  and in GTK. One TypeScript app now covers both platforms (and Windows later, if wanted).
- The iPhone app has no such alternative and keeps SwiftUI, so `kit/` lost its second
  platform and became the iPhone app's own package.

## Layout

| Path | |
|---|---|
| `src/main/` | Main process: host service (`host-controller.ts`), loopback HTTP/SSE proxy (`host-proxy.ts`), menu bar/tray, windows, account sign-in (Clerk Frontend API, `account.ts`), cloud API and device key (`cloud.ts`), updates (`updates.ts`), SSH tunnels (`ssh.ts`), Codync Screen agent (`screen.ts`), on-device speech (`speech.ts`), browser sign-in callbacks (`auth.ts`) |
| `src/preload/` | The `window.codync` bridge (types in `src/shared/ipc.ts`) |
| `src/renderer/store/` | `BotStore` (port of kit's; `bot-store.ts` actions over `bot-sync.ts` events stream and `bot-mirror.ts` state), `AppModel` (window state, approvals), `CloudModel` (claims, cloud default), `AccountSession` |
| `src/renderer/components/` | Design system: controls, overlays (sheets, dialogs, menus), SF Symbol icons, character avatars, thinking orbs |
| `src/renderer/views/` | Screens, mirroring kit's folders (`thread/`, `bots/`, `marketplace/`, `settings/`, `routines/`, `call/`) |
| `native/speech-macos/` | `codync-speech`, a Swift helper running SFSpeechRecognizer for on-device calls |
| `tools/export-symbols.swift` | Renders the SF Symbols the app uses into masks at build time (macOS) |

The renderer owns all state; closing the window hides it, so the menu bar keeps its data.
HTTP to the host leaves from the main process (the loopback API has no CORS).

Unsent composer text is kept per computer, separately for every bot and reply thread, and
saved to `localStorage`, so switching conversations, closing a reply panel or restarting the
app restores the exact text. Submitting clears only that conversation's draft. Drafts are
cleared when the host identity changes or the bot is deleted. iOS and the TUI keep drafts the
same way; the shared desktop behavior covers macOS and Linux.

When several computers are selected, the sidebar groups bots by computer, with connection
status and update notices in each section. Drag a heading or press Alt+Up/Down on the
heading to reorder sections. Click a heading to fold its bots.
Order and folded sections stay on this device; the iPhone and other computers keep
independent orders even when signed in to the same account. Filtering a computer out
does not discard its position. One selected computer keeps a flat roster. Bot pin/activity
ordering stays within each section, and arrow-key navigation skips folded sections.
Search still includes their bots. The compact sidebar uses computer badges for the same
section controls. Ordering is saved locally immediately and survives restart, with separate
preferences per account and cloud environment. See [computer ordering](../features/computer-order.md).
The TUI connects to one host at a time, so it keeps its single roster.

## Platform notes

- **SSH startup** in the shared desktop UI keeps the workspace open whenever saved profiles
  exist. With no online computer, the detail pane shows each profile's connection
  progress, failures, reconnect and host-key confirmation controls, plus local host
  install/retry actions. Manage computers stays reachable throughout. The computer
  filter distinguishes connecting, attention, disconnected and opening states and
  keeps local recovery actions reachable when a remote computer is already online.
  iOS uses direct/encrypted host connections, and the TUI connects to one host;
  neither manages desktop SSH profiles. Windows exposes the same profile UI, but
  the tunnel backend still uses `/usr/bin/ssh*` and Unix socket ownership checks;
  Windows tunnel support has not been implemented.
- **Icons**: SF Symbols may only ship in apps for Apple platforms, so the masks are generated
  on macOS at build time and never committed; Linux draws the closest Lucide icons.
- **On-device speech** is macOS only; Linux calls use OpenAI or Gemini on the user's key.
- **Host**: the Mac app bundles `codync-host` in `Contents/Resources`; Linux uses the installed
  host (an AppImage's mount path changes on every launch, so a service can't point into it).
- **Windows** (x64, per-user NSIS install, unsigned for now so SmartScreen warns once) bundles
  `codync-host.exe` and `codync-hostw.exe` in `resources/`. `codync-host install` writes the
  per-user Run key (`HKCU\…\Run\CodyncHost`), which starts `codync-hostw.exe` at sign-in: a
  windowless launcher that runs the host with a hidden console (the agents share it, so none
  opens a window) and restarts it when it exits. App updates stop the host first, as on macOS.
  Agents start through `cmd /c` (`host/src/shell.rs`), setup terminals use ConPTY
  (`portable-pty`), and the vault key lives in Credential Manager. Not on Windows yet: Remote
  screen and the `computer` MCP (no screen helper), on-device speech, the Claude Code status
  line hook (usage still comes from `claude -p /usage` and ACP), SSH tunnels to other
  computers, and `curl | bash` installers (npm installs and Claude Code's PowerShell installer
  work). The tray icon is the app icon; left-click opens the chat, right-click the menu.
- **Remote screen** on macOS registers `CodyncScreen.app` (built from the Xcode `Screen` target)
  as an SMAppService agent; Linux starts its helper from the host, as before.
- **Other computers in the account** are reached through this computer's own host and SSH
  tunnels only. The encrypted channel to account computers (`ChannelTransport`) wasn't ported:
  the product targets one computer, so "Ask for access" rows show status only.
- **Updates**: electron-updater from GitHub releases (`latest-mac.yml` carries `minApp`, the same
  iPhone gate as before). Installs of the SwiftUI app get the first desktop release through one
  more Sparkle appcast, generated for the zip by the release workflow (deprecated, removed after
  2026-11-06: [updates guide](../guides/updates.md#deprecated-the-sparkle-migration-appcast-remove-after-2026-11-06)).

## Development

SSH host discovery preserves the remote login shell's `PATH`, then searches
`/opt/homebrew/bin`, `/usr/local/bin`, `/home/linuxbrew/.linuxbrew/bin`,
`~/.local/bin`, and `/Applications/Codync.app/Contents/Resources` (Electron) or
`Contents/MacOS` (older native apps). Custom prefixes or relocated app bundles
must expose `codync-host` through the remote login shell's `PATH`.
Managed tunnels use `ssh -S none` to bypass connection sharing: the app's SSH
child must own the listening socket and stay alive until disconnect. This keeps
`ControlMaster` / `ControlPersist` in the user's config from transferring the
forward to a shared master that outlives the app.

For a read-only macOS integration check against an already trusted SSH computer,
run `node --experimental-strip-types tools/ssh-e2e.mjs <ssh-alias>` from
`apps/desktop`. It checks discovery, listener ownership, host identity, API
authentication and sync, SSE, disconnect/reconnect, and an occupied local port.
It uses port 19222 on the remote computer and leaves bots and settings unchanged.

```sh
cd apps/desktop
npm ci
npm run icons                       # macOS: SF Symbol masks
npm run dev                         # Codync Dev, separate host on 19223
CODYNC_PORT=19333 CODYNC_HOME=/path/to/home npm run dev   # against a manually run host
npm run typecheck && npm test
```

UI checks: `CODYNC_SHOW_INACTIVE=1` opens the window without taking focus,
`CODYNC_DEBUG_OPEN=compose|group|plugins|computers|<bot name>` opens a screen, and
`CODYNC_REMOTE_DEBUG=<port>` lets a script drive the renderer over the DevTools protocol.
Each packaging script names its environment:

```sh
npm run dist:mac:dev      # dev cloud, Clerk development
npm run dist:mac:main     # main: what the release workflow ships
npm run dist:linux:dev    # / dist:linux:main
npm run dist:win:dev      # / dist:win:main, on Windows (builds the bundled host with rustup)
```

They write `resources/account-config.json` for that environment, and the Mac ones build the
bundled host (`tools/native-host.mjs`, universal, through rustup's pinned toolchain) for the same
one, so app and host always match, plus the Codync Screen helper (`tools/native-screen.mjs`).
Extra electron-builder flags go after `--` (`npm run dist:mac:dev -- --dir --arm64`). The speech
helper at the top of `electron-builder.yml` doesn't depend on the environment; place it first
(the release workflow, `.github/workflows/release-desktop.yml`, shows how).
`dist:mac:dev` signs with the team 7FUM8A8H72 development certificate (`CSC_NAME` in
`package.json`): the Screen launch agent only runs code from that team, and the keychain holds
another team's certificate electron-builder would otherwise pick. Update the name when it is
renewed.
Codync and Codync Dev can run side by side: separate app identities, host services, ports, data and login callbacks. See [Dev app isolation](../guides/dev-app.md) for the resource map and external signing/account setup.

For a local signed Mac build, explicitly select a development identity from the project's
team with `-c.mac.identity="Apple Development: …"`; use `--dir --arm64` and
`-c.mac.notarize=false` only for local development. Automatic identity discovery can select
an old certificate from another team. `security find-identity` alone does not establish that
a certificate is still trusted: verify its exported public certificate with
`security verify-cert -c certificate.pem -p codeSign -R ocsp -R require` before signing.
If Gatekeeper reports `CSSMERR_TP_CERT_REVOKED`, fix the signing identity rather than
disabling security checks. Distributed builds still require Developer ID signing and notarization.

## Parity with the SwiftUI app

The port follows the Swift sources view by view (metrics, fonts, colors, motion), including the
Mac chat redesign that was in progress at the time (larger chat text, actions beside messages,
one composer capsule, circular chat controls, Jump to latest). It was checked side by side with
the SwiftUI build against the same host using screenshots from Computer Use.

Composer keyboard behavior, animation and durable draft storage, with a host-free
regression fixture: [composer checks](../guides/composer.md).

## The SwiftUI apps in the history

`apps/macos/`, `apps/linux/` and the Mac code paths in `kit/` are in Git. The commit that
removed them is `git log --oneline -1 -- apps/macos`; its parent is the last SwiftUI version:

```sh
git show <commit>^:apps/macos/Views/ChatWindow.swift
git worktree add ../codync-swiftui <commit>^
```
