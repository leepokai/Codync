# File structure and naming

This is the map of the current repository. Paths below are repository-relative. Start with the [architecture overview](overview.md) for runtime relationships.

## Top-level layout

```text
Codync/
├── host/                      # Rust daemon and terminal client
│   ├── src/
│   │   ├── main.rs            # CLI entry, shared helpers (LockExt, http)
│   │   ├── *_cli.rs           # Subcommands with their own module: cloud, network (+ network_cli/setup.rs: `network setup`), send (+ send_cli/: event feed, --wait)
│   │   ├── hub.rs, store/     # Shared state + event fan-out / SQLite persistence (schema, model, bots, entries, devices)
│   │   ├── service/           # Data dir, launchd/systemd/Windows install, keep-awake
│   │   ├── usage/, mcp.rs     # usage/: which agents' plan limits are read and shown (readers: crates/agent/src/usage/); mcp.rs: built-in stdio MCP servers; mcp/remote.rs: remote connector proxy
│   │   ├── screen/            # Remote screen: helper protocol, helper socket, viewers (phones, desktop app), bot computer use (cua-driver)
│   │   ├── api/               # Dispatch (mod.rs), HTTP/SSE routes (http.rs, events.rs), method groups (bots.rs, host.rs, marketplace.rs, remote.rs), caller permissions (devices.rs)
│   │   ├── agent/             # ACP client, harness discovery, registry, sign-in, setup PTYs, bot actor, new bot defaults (new_bot.rs)
│   │   ├── chat/              # Groups, bot-to-bot requests, prompt snapshots, memory
│   │   ├── remote/            # Identity, wire crypto, E2E channel, cloud, relay socket, push
│   │   ├── market/            # Marketplace (registry.rs, install.rs, skills.rs), Composio, MCP OAuth
│   │   └── tui/               # Terminal client (app/: state and input, the bot form in forms.rs + approvals.rs, the group form in group_form.rs; view/: drawing; manage/ + sheets.rs: settings; connections.rs: secure connection requests)
│   └── tests/                 # Host integration tests and scripted ACP agents
├── apps/
│   ├── project.yml            # XcodeGen source of truth for every Apple target
│   ├── Codync.xcodeproj/      # Generated Xcode project
│   ├── shared/Config/         # {dev,main}.plist: environment for the iPhone, Android and desktop apps
│   ├── ios/                   # App/, Views/, Resources/, Widgets/, NotificationService/, Share/ (share sheet), WhatsNew.md (App Store notes)
│   │   └── Kit/               # The iPhone app's Swift package
│   │       ├── Sources/
│   │       │   ├── CodyncKit/  # Wire models, transports, design; widget-safe
│   │       │   │   ├── Client/
│   │       │   │   ├── Models/
│   │       │   │   ├── Design/
│   │       │   │   └── Resources/
│   │       │   └── CodyncUI/   # Stores, screens and custom controls
│   │       │       ├── Store/
│   │       │       ├── Bots/
│   │       │       ├── Thread/
│   │       │       ├── Call/
│   │       │       ├── Screen/
│   │       │       ├── Marketplace/
│   │       │       ├── Routines/
│   │       │       ├── Usage/
│   │       │       └── Resources/
│   │       └── Tests/         # CodyncKitTests and CodyncUITests
│   ├── desktop/               # Electron app for macOS, Linux and Windows (see desktop-app.md)
│   │   ├── src/main/          # Host service, tray, account, updates, SSH, screen, speech
│   │   ├── src/preload/       # window.codync bridge
│   │   ├── src/renderer/      # React UI: store/, components/, views/
│   │   └── native/            # codync-speech (macOS on-device speech helper)
│   ├── android/               # Android phone app (Kotlin, Compose), ported from the iPhone app; WhatsNew.md (Google Play notes)
│   │   ├── core/              # JVM models, crypto, pairing, encrypted channel, host and cloud clients
│   │   ├── design/            # Codync's components: palette, type, motion, glass, sheets, menus, dialogs, controls, symbols
│   │   ├── app/               # Screens, the stores (AccountStore, one AppStore per computer), notifications, Live updates, Glance widgets
│   │   └── tools/             # symbols.py (SF Symbol stand-ins), launcher_icon.py, agent_icons.py
│   ├── screen-macos/          # macOS screen helper (Xcode Screen target)
│   ├── screen-macos-tests/    # Swift package testing the helper's socket lifecycle and candidate rules
│   └── screen-linux/          # Linux screen helper: portals or a plain X display, GStreamer
├── cloud/                     # Account API + encrypted relay Worker and Durable Object
│   ├── src/
│   ├── migrations/            # D1 migrations
│   └── test/                  # workerd tests; e2e/ runs against a real test host
├── private-relay/             # Single-process relay (the cloud WebSocket subset) for a person's own computers
├── relay/                     # Separate APNs push Worker; src/ and test/
├── discord/                   # Discord GitHub-role Worker (discord.codync.dev)
├── docs/                      # Architecture, features, design, guides, reference, upstream agent bugs, plans, investigations, archive
├── tools/                     # Release, App Store and Google Play submission (store_release.py: what both share), font scripts, xcodebuild.sh (shared compile cache)
├── packaging/                 # Distribution templates
├── marketing/app-store-screenshots/ # App Store screenshot editor (bun dev; Export bundle → out/)
├── web/                       # Website (Next.js static export)
└── .github/workflows/         # CI, signing and release automation
```

`build/`, `apps/ios/Kit/.build/`, `apps/desktop/out/`, `apps/desktop/dist/`, Cargo `target/`, `node_modules/` and Wrangler local state are generated working data. They are not source modules and should not become documentation locations. Edit `apps/project.yml` and run `xcodegen generate --spec apps/project.yml`; never hand-edit `project.pbxproj` (`tools/bump-version.py` sets its `MARKETING_VERSION` lines to what xcodegen would write).

## Targets

- `apps/desktop/` — desktop app for macOS, Linux and Windows (Electron; `npm run dev`, packaged by `.github/workflows/release-desktop.yml`)
- `iOS` (`apps/ios/`) — iOS app
- `Screen` (`apps/screen-macos/`) — Codync Screen: capture, input and WebRTC for Remote screen, embedded in the macOS desktop app (`Contents/Library/LoginItems`); starts the computer-use driver it carries in `Contents/Helpers`
- `apps/screen-macos-tests/` — native helper tests: socket lifecycle and candidate rules (`swift test --package-path apps/screen-macos-tests --scratch-path build/dd/screen-macos-tests`)
- `apps/screen-linux/` — `codync-screen` (Rust, GStreamer + xdg portals, or `ximagesrc` + XTest on a plain X display: `backend.rs`, `x11/`), the Linux Remote screen helper
- `Widgets` (`apps/ios/Widgets/`) — usage widget + bot Live Activity (bundle id `com.pokai.Codync.ios.LiveActivity`)
- `apps/android/` — Android app (`com.pokai.Codync.android`, Dev `.dev`; Gradle modules `app`, `core`, `design`; CI `.github/workflows/android.yml`)
- `CodyncKit` (`apps/ios/Kit/`) — the iPhone app's Swift package: `CodyncKit` + `CodyncUI` libraries (tests: `../../../tools/xcodebuild.sh test -scheme CodyncKit-Package` from `apps/ios/Kit` on an iOS simulator)

## Where to make a change

| Responsibility | Source entry points |
|---|---|
| CLI and background service | `host/src/main.rs`, `service/`, `watchdog.rs` (a host that stops answering is restarted); subcommands `cloud_cli.rs`, `network_cli.rs` (+ `network_cli/setup.rs`, `setup/hooks.rs`: `network setup` writes the MCP server and hooks into each coding agent's config), `send_cli.rs` (+ `send_cli/events.rs`, `wait.rs`) |
| API, caller permissions, event ordering | `host/src/api/`, `hub.rs` |
| Workflows (a graph on this computer, separate from a bot's routines) | `host/src/workflows/`; desktop `apps/desktop/src/renderer/views/workflows/`; iPhone `apps/ios/Kit/Sources/CodyncUI/Workflows/`; Android `apps/android/app/` workflow screens; terminal `host/src/tui/manage/workflows.rs`, `view/workflows.rs` |
| SQLite transcript, bots, lanes and sessions | `host/src/store/` (`entries.rs`, `bots.rs`, `model.rs`, `devices.rs`, `schema.rs`) |
| Agent process, ACP, queue and session lifecycle | `host/src/agent/bot/` (`launch.rs`, `queue.rs`, `session.rs`, `steer.rs`, `turn.rs`, `updates.rs`; how Codex and pi get Codync's MCP servers is `host/crates/agent/src/mcp_names.rs` and `pi.rs`), `acp.rs` (+ `acp/codebuddy.rs`, CodeBuddy's ACP extensions), `process.rs` (process-tree ownership; `process_unix.rs` on macOS and Linux, `process_windows.rs` on Windows) |
| Compaction timing and rollover | `host/src/agent/bot/compaction.rs`, the handoff card in `host/src/chat/handoff.rs`; the rules in `codync-mem`'s `compaction.rs` |
| Group room turns / bot-to-bot requests | `host/src/chat/group.rs` (+ `group/rooms.rs`) / `team.rs` |
| Work: tasks, threads and the event box (the dispatcher, steering, a thread's transcript) | `host/src/chat/work/` (`mod.rs` a thread's turn end, the report, host start; `tasks.rs` the box and its wire shape; `dispatch.rs`; `steer.rs`; `tools.rs`; `recall.rs` `recall_task`; `prompts.rs`; `sources.rs` routine runs and bot requests as tasks; `thread.rs` the user's messages to a task's thread; `sweep.rs` the hourly look; `watch.rs` the watchdog for a thread that looks stuck; `ask.rs` `ask_task`; `ask_chat.rs` a thread's `ask_chat` and the bot's `answer_task`; `plain.rs` threads without a bot and their own folders; `archive.rs` older threads: tasks out of the box, parked thread bots, `threadKeepDays` and `olderThreads`, stored in `host/src/store/archive.rs`; `worktree.rs` a thread's git worktree, git itself through `host/src/git.rs`, and `fill.rs` what a new worktree is given), the fork in `host/src/agent/bot/session.rs`; desktop `apps/desktop/src/shared/work.ts`, `threads.ts`, `src/renderer/store/older-threads.ts` (older threads paged for the Threads list, and the Delete old threads choices), `src/renderer/views/chat/WorkCapsule.tsx`, `WorkTimeline.tsx`, `WorkRows.tsx` (the Threads and Event box rows, shared with the Work sheet), `WorkDialogs.tsx`, a task's thread in the chat's form `ThreadChat.tsx` (on the Threads page and as a tab of the inspector) with its pieces in `WorkThreadTranscript.tsx`, its copyable where line `WhereCopy` in `WorkWhere.tsx`,  the git state under a bot's folder `src/renderer/views/bots/GitStateLine.tsx`, and the sheet of all bots' work `src/renderer/views/AllWorkView.tsx`; iPhone `apps/ios/Kit/Sources/CodyncKit/Models/WorkTask.swift`, `OlderThreads.swift` (a page of older threads and the list paged so far), `ThreadKeep.swift` (the Delete old threads choices), `CodyncUI/Store/BotStore+OlderThreads.swift` (loading them), `WorkThread.swift` (a thread's rules: who takes messages, the where line, the git state), `CodyncUI/Bots/WorkTimeline.swift`, `CodyncUI/Bots/GitStateRow.swift`, `CodyncKit/Models/ThreadSections.swift` (the Threads list's sections and grouping), `CodyncUI/Bots/ThreadList.swift` (the Threads tab's list and row menu), `CodyncUI/Chat/NewThreadView.swift` (the Threads tab's New thread page, which becomes the thread's chat once sent), `CodyncUI/Bots/ListSearch.swift` (the Bots, Threads and Workflows tabs' search field), `CodyncUI/Bots/WorkQuestion.swift` (Stop's and Cut in's dialogs, shared with the details' rows), `CodyncUI/Chat/WorkCapsule.swift`, `ThreadChatView.swift` (the task's thread in the chat's form, pushed on the Threads tab or inside the details sheet), `CodyncUI/Store/BotStore+Thread.swift`; Android `core/` `Work.kt`, `WorkThreads.kt` and `app/` `WorkCapsule.kt`, `WorkTimeline.kt`, `WorkRows.kt`, `WorkWords.kt`, `AppStoreWork.kt`, `AppStoreThreads.kt`, `ThreadChat.kt` (the task's thread in the chat's form, pushed on the Threads tab or inside the details sheet), `ThreadsView.kt` (the Threads tab's list, and the thread chat pushed over it), `NewThread.kt` (its New thread), `ListHeader.kt` (header buttons and the list tabs' header: account, title and subtitle, search field); terminal `host/src/tui/app/work.rs`, `app/thread_keep.rs` (the palette's Delete old threads), `manage/work.rs`, `manage/older.rs` (a bot's older threads under Recent, a page at a time), `manage/threads.rs` (a task's thread: its keys and composer), `view/work.rs`, `view/thread.rs` (the thread page), `sheets/details_view.rs`, and the Work page `manage/all_work.rs` + `sheets/all_work_view.rs` |
| Voice calls (the call, the voice's instructions and tools, transcripts, call lines) | `host/src/chat/calls/` (`mod.rs`, `context.rs`, `project.rs` the git state, `tools.rs`, `announce.rs` what the voice hears while the call lasts, `speech.rs` text as a voice says it); the user's messages it relays go through `host/src/chat/messages.rs` like typed ones; clients' `CallLink` (iPhone `CodyncUI/Call/CallLink.swift`, desktop `views/call/call-link.ts`, Android `core/Calls.kt` + `HostCallLink.kt`) and call rows (`CallRows`); the call on the iPhone's Lock Screen (`CodyncUI/Call/CallActivity.swift`, the Live Activity) and Android's call notification (`VoiceService.kt`) |
| Dev servers (programs a bot keeps running) | `host/src/dev_servers/` (`run.rs` starting and stopping, `scan.rs` restoring and reading ports, `ports.rs`, `tools.rs` the bot's tools, `logs.rs`, `detected.rs` what a bot left running through its shell, `platform.rs`); desktop `views/chat/DevServersSection.tsx` (+ `src/shared/dev-servers.ts`); iPhone `CodyncUI/Bots/DevServersSection.swift`; Android `DevServersSection.kt`; terminal `host/src/tui/manage/dev_servers.rs` and `view/dev_servers.rs` |
| Team management (bots creating and changing bots) | `host/src/chat/manage.rs`, its approval card (`Cmd::Approve`) in `agent/bot/updates.rs` |
| codync-network (your computers' network) | `host/crates/network` (the crate: roster, letters, reach, budgets, `rules.rs`; `node/endpoint.rs` for where a computer is; `sessions.rs` which coding agent's session an MCP server serves), `host/src/network/` (one node per network in `mod.rs`, the saved networks in `saved.rs`, its bots' address names in `handles.rs`, coding agents' sessions and their mail in `sessions.rs`, the `network*` methods, `networkView` and the `network` event in `api.rs`), `host/src/network_cli.rs` (+ `network_cli/setup.rs`, `setup/hooks.rs`: each coding agent's MCP server and hooks); across-computer bot requests in `host/src/chat/team/remote.rs`, a session's mailbox tools in `host/src/chat/team/mailbox.rs`; desktop `apps/desktop/src/renderer/views/settings/NetworkSection.tsx`, `NetworkCard.tsx`, `NetworkSheets.tsx`, `RuleSheet.tsx`, `SessionsCard.tsx` (coding agents' sessions; e2e `e2e/network-sessions.spec.ts`) and the bot editor's `views/bots/NetworkSwitches.tsx` (+ `store/network.ts`, `lib/network-copy.ts`, `src/shared/network.ts`); iPhone `apps/ios/Kit/Sources/CodyncUI/NetworkSheet.swift` (+ `CodyncKit/Models/Network.swift`); Android `NetworkSheet.kt`, `AppStoreNetwork.kt` (+ `core/Network.kt`); terminal `host/src/tui/manage/network.rs`, `network_setup.rs`, `network_rules.rs` and `view/network.rs`, `view/network_rules.rs` |
| Prompt snapshots and memory | `host/src/chat/context.rs`, `memory/` (`api.rs` the apps' read-only view, `tools.rs` and `file_tools.rs` the MCP tools, `files.rs` the folders and scan, `turns.rs` which bot's turn a found change is credited to, `file_api.rs`, `dream.rs`, `import.rs`); the rules in `host/crates/memory` (`codync-mem`: files and facts in `files/`, dreaming in `dream/`, tombstones in `store.rs`, the old entries in `legacy.rs`); harnesses' own memory turned off in `host/src/agent/own_memory.rs` (each harness's switch in `host/crates/agent/src/own_memory.rs`) |
| Identity, encryption and direct channel | `host/src/remote/identity.rs`, `crypto.rs`, `channel.rs` |
| Host cloud state and relay connection | `host/src/remote/cloud.rs`, `cloud_url.rs`, `relay.rs` |
| Codync Cloud computers (host side: self-claim, sleep, cloud updates, its name; the keeper that holds its secrets) | `host/src/remote/claim.rs`, `managed.rs`; `host/src/keeper/` (`store.rs` sealed secrets, `socket.rs` control socket, `proxy.rs` the local proxy that adds keys, `unlock.rs` its key from the cloud, `update.rs`, `client.rs` the host's side, `protocol.rs` what they say); Worker `cloud/src/routes/cloud-computers.ts`, `cloud-keeper.ts`, `cloud-host.ts`, `cloud-setup.ts`, providers in `cloud/machines/` |
| The agents themselves, with no app in it (the `codync-agent` crate; the host calls `codync_agent::init` at start) | `host/crates/agent/src/`: `backends.rs` (+ `backends/catalog.rs` the known harnesses, `backends/sign_in.rs`, `backends/search_path.rs`, `backends/workbuddy.rs` the CLI inside the WorkBuddy app), `registry.rs`, `node.rs` Node.js for npm agents, `acp.rs` (+ `acp/capabilities.rs` what `initialize` says the agent can do, `acp/request.rs` the agent's requests, answered by themselves when dropped, `acp/session.rs` a session's prompt, cancel and steer, `acp/codebuddy.rs`, `acp/exit.rs`), `events.rs` the typed `session/update` events (+ `events/claude.rs` what Claude's adapter adds in `_meta.claudeCode`: a call's subagent launch and what it reported), `process.rs` (+ `process_unix.rs`, `process_windows.rs`) the tree an agent starts, `modes.rs`, `config.rs`, `shell.rs` the platform shell, `accounts.rs` more than one login per agent (`accounts/agents.rs` how each agent keeps a login in a folder, `accounts/slot.rs` what the folder shares, `accounts/identity.rs` who it signs in), `codex_app.rs` Codex's own API (`codex app-server`), `usage.rs` each agent's plan limits (`usage` feature: `usage/<agent>.rs` one reader per agent, `usage/command.rs` running a CLI, `usage/acp_read.rs` an ACP slash command, `usage/reading.rs` one reading at a time and where one stopped) |
| Agent sign-in, setup terminal, other logins | `host/src/agent/auth.rs` (+ `auth/agent.rs` starting an agent and its own sign-in, `auth/models.rs`, `auth/model_cache.rs` the lists kept per login; reading the advertised methods and models is `host/crates/agent/src/acp/auth.rs` and `acp/models.rs`), `login_profiles.rs` (an agent's other logins: `login_profiles/account.rs` the account kind's folder and sign-in, `login_profiles/keys.rs` the API key kind's variables), `term.rs` |
| Fast mode (session config option) | `host/crates/agent/src/config.rs`, `agent/bot/fast_mode.rs` |
| Agent modes (Permissions) | `host/crates/agent/src/modes.rs` (the modes, Auto, `switch`), `agent/bot/modes.rs` (each session's mode), `agent/auth/models.rs` (`agentModels`, `InFlight`); terminal `host/src/tui/app/approvals.rs`; iPhone `CodyncKit/Models/AgentMode.swift`, `CodyncUI/Bots/ApprovalPicker.swift`; Android `core/Approvals.kt`, `ApprovalRow.kt`; desktop `src/shared/approvals.ts`, `views/bots/ApprovalRow.tsx` |
| Marketplace, Composio, connector OAuth | `host/src/market/` |
| Skills a bot writes (`save_skill`) | `host/src/chat/skills.rs`, stored by `host/src/market/skills.rs` |
| Artifacts (`publish_artifact`) and a chat's Library | `host/src/chat/artifacts.rs`, `library.rs`; desktop `apps/desktop/src/renderer/views/library/` (+ `src/main/artifacts.ts`); iPhone `apps/ios/Kit/Sources/CodyncUI/Artifacts/` (+ `Store/ArtifactCache.swift`, `BotStore+Artifacts.swift`); Android `ArtifactCardView.kt`, `ArtifactViewer.kt`, `ArtifactWebView.kt`, `LibraryView.kt`, `AppStoreArtifacts.kt` (+ `core/Artifact.kt`, `core/Library.kt`); terminal `host/src/tui/net/artifacts.rs`, `manage/library.rs` |
| Screen bridge and built-in MCP tools | `host/src/screen/`, `mcp.rs` |
| Swift transport and cloud API | `apps/ios/Kit/Sources/CodyncKit/Client/` |
| Account aggregation / one host mirror | `apps/ios/Kit/Sources/CodyncUI/Store/AccountStore.swift` / `BotStore.swift` (+ `BotStore+Connection`, `+Sync`, `+Sending`, `+Actions`, `+Plugins`, `+Cache`, `+Shortcuts`) |
| iPhone chat, replies, composer and trace | `apps/ios/Kit/Sources/CodyncUI/Chat/` |
| Reusable iPhone UI chrome | `apps/ios/Kit/Sources/CodyncUI/Chrome.swift`, `Controls.swift`, `MenuItem.swift` (system menus: `MenuItem`, `.contextActions`, `DropdownMenu`), `Platform.swift` |
| iOS navigation, pairing, account, settings, computer management | `apps/ios/Views/RootView.swift`, `BotListView.swift`, `ThreadsTab.swift`, `ChooseComputerView.swift`, `PairingView.swift`, `AccountSwitcherView.swift`, `SettingsView.swift`, `ComputersView.swift`, `ComputerConnectionRows.swift` |
| Apple account sessions / public environment config | `apps/ios/App/AccountSession.swift`, `apps/shared/Config/` |
| Desktop local host / SSH lifecycle | `apps/desktop/src/main/host-controller.ts`, `ssh.ts` |
| Desktop store, chat and controls | `apps/desktop/src/renderer/store/bot-store.ts` (actions; state in `bot-mirror.ts`, events stream in `bot-sync.ts`), `views/chat/`, `components/` |
| Cloud routes / authentication / relay | `cloud/src/index.ts`, `api.ts`, `routes/`, `auth.ts`, `relay.ts` |
| Cloud assistants (the MCP connector at mcp.codync.dev) | `cloud/src/routes/assistant.ts` (MCP, OAuth metadata, routing to computers), `host-calls.ts` (the relay DO's calls to its host); `host/src/chat/team/assistant.rs` (the setting, the tools, the replies); the switch in desktop `apps/desktop/src/renderer/views/settings/ComputersView.tsx` (+ `store/cloud.ts`) and terminal `host/src/tui/manage/network.rs`; shown read-only in the iPhone and Android network sheets |
| MCP access (outside apps driving threads, routines and workflows) | `host/src/chat/team/control/` (levels and tool classes `access.rs`, apps and tokens `clients.rs`, tools `threads.rs`, `runs/`, `capabilities.rs`, schemas `schema.rs`), `host/src/chat/work/external.rs` (threads an app starts), `host/src/api/mcp.rs` (the management methods); desktop `apps/desktop/src/renderer/views/settings/McpAccessSection.tsx` + `lib/mcp-access.ts`; terminal `host/src/tui/manage/mcp_access.rs` + `tui/view/mcp_access.rs`; the cloud copy of the tools `cloud/src/routes/assistant-tools.ts` |
| Push encryption / APNs delivery / decryption | `host/src/remote/push.rs`, `relay/src/`, `apps/ios/NotificationService/` |
| Widget and activity rendering | `apps/ios/Kit/Sources/CodyncKit/Design/`, `apps/ios/Widgets/` |
| Android screens (one file per iOS view: `BotListView.kt`, `ChatScreen.kt`, `BotDetailsView.kt` (+ `BotSettingsForm.kt`, `BotTemplateView.kt`), `ChooseComputerView.kt`, `PairingView.kt`, `SettingsView.kt`, `ComputersView.kt`, …) | `apps/android/app/src/main/kotlin/com/codync/android/` |
| Android components and glass | `apps/android/design/` (`Controls.kt`, `Chrome.kt`, `Sheet.kt`, `Menus.kt`, `Dialog.kt`, `Glass.kt`) |
| Android transport, pairing and wire models | `apps/android/core/` (`EncryptedChannel.kt`, `HostConnector.kt`, `Pairing.kt`, `Compatibility.kt`) |
| Android widgets, Live updates, push | `apps/android/app/` (`CodyncWidgets.kt`, `WidgetCards.kt`, `LiveUpdateGallery.kt`, `PushService.kt`) |

## Dependency rules

- `CodyncKit` owns serializable models, clients and rendering primitives shared with widgets. It must not import `CodyncUI` or ClerkKit.
- `CodyncUI` owns the iPhone app's observable stores and screens. App-specific lifecycle, OAuth configuration and device hooks belong in `apps/`.
- Each `BotStore` (Swift, and its TypeScript port in the desktop app) talks to one computer. `AccountStore` aggregates stores and routes by `BotReference`; bare bot IDs are not globally unique.
- The host owns routing, reply quotes, permissions and group turn scheduling. Clients render these results; they do not reimplement host policy.
- Android: `core` is plain JVM (no Android or Compose imports) and `design` holds no I/O; `app` uses both. Like the other clients it renders host state and sends intents, never product logic.
- `cloud/` transports encrypted chat traffic and manages account metadata. `relay/` delivers APNs pushes. The two Workers have separate configuration and tests. `private-relay/` is a single-process subset of the cloud relay protocol for a person's own computers. It is not the APNs worker.

## Names

One thing has one word: on screen, in the user manual, in what a bot is told, in code and on the wire. The
"Never" column lists the words that word replaces; a new name for something here is a change to this table first.

| Term | Meaning | Never |
|---|---|---|
| bot | What the user builds and talks to: one agent combined with a profile (name, avatar, instructions), working folder, memory, skills, routines and connectors. A group is also a roster entry with `kind: group` | agent, assistant |
| lead bot | The bot the user reaches first: calls, Siri and shortcuts without a bot go to it, and it manages the others | lead agent |
| agent | A coding tool Codync composes into bots (Claude Code, Codex, …) and its ACP adapter. Code that picks and starts one says `backend` | harness |
| `agent@computer` | A codync-network address: whoever can take a letter there, whether a bot, a bare agent session (Claude Code, Codex) or a person at a terminal | |
| chat | Main, persistent conversation with a bot or group | conversation, main chat, coordinator; `Thread*` in code |
| turn | One message to a chat or a thread, up to the end of the bot's answer to it | task |
| work | Everything a bot does away from its chat: its tasks and the threads that do them. Uncountable; the Work page and the capsule above the composer show it | background (as a noun), "a work" |
| task | One piece of work, handled on its own: `title`, `brief`, where it came from, a status (`waiting`, `working`, `needsYou`, `done`, `failed`, `stopped`) and, once done, its report. It exists from the moment it is put in the event box | item, job; task for a turn of the chat |
| thread | Where the bot does tasks away from the chat, started for them. Like the chat, it has its own agent process, session and transcript. It does one task at a time and may go on to the next; the user can write to it. A task that waits has no thread yet | task (the old code name), agent, session |
| event box | Where a bot's tasks wait until a thread takes them | box alone on screen, queue |
| threads at once | How many threads one bot runs at the same time (`maxThreads`) | place, slot, agents at once |
| brief | What a task's thread is told to do; written by the bot, a routine or another bot | prompt |
| report | What a thread hands back when its task ends; the bot turns it into a message in the chat | result (the field inside a report keeps the name) |
| reply | A user message answering an earlier message of its chat, with that message's quote | |
| group | Chat with several bots and the user; no agent process of its own | group chat as a different thing, team |
| section | A named part of the bot list | folder |
| lane | Destination of a turn: a bot's own chat or a group's | |
| session | What the agent remembers right now of a chat or a thread: its ACP context. Each chat and thread has one at a time and keeps going when it gets a new one ([when](../features/context-and-memory.md#transcript-context-and-memory)); its transcript stays. A routine run in the chat gets one of its own; a group has none. Codex's own word for a session is thread | conversation, thread |
| steps | What a bot thinks and which tools it calls inside one turn | thinking details |
| trace | Full chat, steps included (on screen: Full conversation) | transcript for the same thing |
| details | The panel beside a chat: details, library, memory, settings | |
| library | A chat's artifacts and shared files | |
| working folder | The folder a bot works in. A **project** is one the user picked; an **own folder** is the one Codync gives a bot without a project | workspace |
| worktree | A git worktree a thread gets when another thread already works in the same repository | |
| approval | A bot asks before it acts; shown as an approval card | permission (for one request) |
| permission | A standing grant: a system one (screen recording) or a bot's permission setting | approval (for a setting) |
| needs you | The state of a bot or task that waits on the user: an approval, a choice or a sign-in | needs input |
| computer / host | User-facing paired environment / the daemon that serves it (on screen only where it is installed or repaired) | machine |
| device | A phone or computer that connects to a computer | |
| cloud / relay | Account + E2E service / context-dependent transport or APNs Worker; always name the directory when ambiguous | |
| cloud computer | A computer started through Codync Cloud | |
| call | Talking to a bot by voice | |
| remote screen / computer use | A person looks at and controls a computer / a bot uses its apps. Computer access is the settings page for both | |
| connector | An MCP server added to Codync; skills and connectors come from Extensions | MCP server (outside the paste-a-config screen), plugin |
| routine / run | Work a bot does on a schedule or a webhook / one time it does it. A run is a task in the event box | scheduled task |
| follow-up | A later turn a bot sets for itself, once | reminder |
| login profile / account | One sign-in or API key of an agent / the user's Codync account | |

Use lowercase repository folders; PascalCase folders inside Swift targets. Swift files follow their main type (`UpperCamelCase.swift`), Rust uses `snake_case.rs` (a folder module is `name/mod.rs`), TypeScript uses `kebab-case.ts`, and React components use `PascalCase.tsx`. Prefer one main type per file; small related view siblings can share a plural file such as `ChatRows.swift`.

Role suffixes: `View`, `Row`, `Card`, `Window`, `Store`, `Controller`; button styles describe the effect (`PressScale`). Keep feature vocabulary aligned across Swift, the desktop app, Rust and the terminal client.

UI-specific rules, including the native iPhone toolbar exception and the desktop controls, live in [UI conventions](../design/ui-conventions.md).
