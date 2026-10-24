# Codync documentation

Current documentation describes the checked-out implementation. Index last checked against the tree: **2026-10-19**.
Source review, a passing build, local integration tests and production acceptance are different checks; a feature documented here is not a claim that its deployed service has been verified.

## Start here

| Task | Read |
|---|---|
| Find a module or decide where a file belongs | [File structure](architecture/file-structure.md) |
| Understand clients, host, cloud and data ownership | [Architecture](architecture/overview.md), [conversation runtime](architecture/conversation-runtime.md) |
| Work on the macOS, Linux and Windows desktop app (Electron) | [Desktop app](architecture/desktop-app.md), [desktop styles](guides/desktop-styles.md) |
| Build, install, restart and run checks | [Development](guides/development.md), [Codync Dev beside production](guides/dev-app.md) |
| Write or refactor code in any module | [Clean code](guides/clean-code.md) |
| Change something users see or do (keep the codync.dev manual current) | [User docs](guides/user-docs.md) |
| Change an iPhone or desktop screen or button | [UI conventions](design/ui-conventions.md) |
| Change the composer | [Composer behavior and checks](guides/composer.md) |
| Ship app and host updates | [Updates](guides/updates.md), [client and host compatibility](reference/compatibility.md) |
| Plan and build Android | [Android plan](plans/android-app.md), [Android status](../apps/android/README.md) |
| Test a phone over Cloudflare instead of LAN | [Cloudflare testing](guides/cloudflare-testing.md) |
| Configure dev/main or deploy the services | [Environments and deployment](guides/environments-and-deployment.md) |
| Run the host on a Linux server or cloud VM | [Linux servers](guides/linux-servers.md) |
| Reproduce or check something on Windows (UTM VM) | [Windows testing](guides/windows-testing.md) |
| Set up accounts, approvals or desktop SSH | [Accounts and SSH](guides/accounts-and-ssh.md) |
| Check connectors against live services | [Connector verification](guides/connector-verification.md) |
| Change transport or cryptography | [Remote protocol](reference/remote-relay.md) and [shared vectors](reference/fixtures/README.md) |
| Look into errors or failed turns users hit | [Usage analytics](features/analytics.md), [iPhone crash tracing](guides/iphone-crash-tracing.md), [investigations](#investigations) |

## Features

- [Chat messages](features/chat-messages.md)
- [Group chats and replies](features/groups-and-replies.md)
- [Bot-to-bot collaboration](features/bot-collaboration.md)
- [Context and memory](features/context-and-memory.md)
- [Bot memory and compaction](design/bot-memory.md) (design; storage, gate and dreaming built, compaction to come)
- [Bot memory research](design/bot-memory-research.md) (files vs. database, embeddings, open-source systems, harness instructions)
- [Bot workspaces](features/bot-workspaces.md)
- [Agent fallbacks](features/agent-fallback.md) (the next agent carries a turn on when the bot's own runs out of usage)
- [Routines](features/routines.md)
- [Voice calls](features/voice-call.md)
- [iOS Shortcuts](features/ios-shortcuts.md)
- [File attachments](features/file-attachments.md)
- [Artifacts and the Library](features/artifacts.md)
- [Remote screen](features/remote-screen.md) and [computer use](features/computer-use.md)
- [Computer identity and list ordering](features/computer-order.md)
- [Notch island (desktop)](features/notch-island.md)
- [Extensions and agent setup](features/marketplace.md)
- [Connector setup and credentials](features/connector-credentials.md)
- [Usage analytics and error tracking](features/analytics.md)
- [Widgets, Live Activities and onboarding](design/mobile-widgets.md)
- [Notifications and background Live Activity updates](design/push-and-live-activity.md)

## Plans

- [Android parity and acceptance](plans/android-app.md)
- [Harness memory](plans/harness-memory.md) (the switch that keeps a harness's own memory on; a choice of it instead of Codync's, planned)
- [Error tracking detail](plans/error-tracking-detail.md) (symbolicated iPhone crashes and sessions, built; consented problem reports, proposed)

## Investigations

Production signals (PostHog errors, failed turns) traced to a root cause, newest first. Dated
records: the fixes they name have shipped or are listed as open.

- [Claude reported signed in on Windows while signed out, 2026-10-20](investigations/windows-claude-sign-in-2026-10-20.md)
- [PostHog error triage, 2026-10-19](investigations/posthog-errors-2026-10-19.md): host panics, desktop and iPhone crashes, failed Claude turns
- [npm agents failing to start on Windows, 2026-10-14](investigations/windows-npm-agents-2026-10-14.md)

## Documentation layout

```text
docs/
├── README.md                  # Navigation and maintenance rules
├── architecture/              # Current boundaries, data flow and file ownership
├── features/                  # Current behavior and implementation entry points
├── design/                    # UI rules, system integration and design write-ups for larger features
│   └── reference/             # Reference screenshots the designs follow
├── guides/                    # Build, configuration, testing and deployment steps
├── reference/                 # Wire contracts and shared executable fixtures
│   └── fixtures/
├── plans/                     # Work planned or in progress, not released yet
├── investigations/            # Dated root-cause records of production errors
├── archive/                   # Dated plans/audits; never current instructions
└── screenshots/               # Images the repository README shows
```

Service-local entry points remain in [cloud/README.md](../cloud/README.md) and [relay/README.md](../relay/README.md). Repository-wide development rules remain in [AGENTS.md](../AGENTS.md) and [CLAUDE.md](../CLAUDE.md).

## Maintaining these docs

1. Describe implemented behavior in the present tense. Label proposals and unverified deployment steps explicitly.
2. Keep one authoritative explanation per topic; link to it from related pages. Link to source for schemas and public APIs instead of copying entire definitions that drift.
3. Use lowercase, kebab-case filenames and relative links. Source paths written in backticks are relative to the repository root.
4. When moving a document, update inbound links, code comments, test fixture imports and CI path filters in the same change.
5. Check local links and run the affected fixture tests. Keep generated images/build logs under `build/` or a temporary directory, outside the documentation tree.
6. Record verification scope and limitations. Do not turn an old test report or historical audit into a current passing status.

[Archived plans and audits](archive/README.md) retain earlier decisions and review context. Their original dates, line numbers and proposed APIs are historical.
