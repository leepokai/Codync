# Codync documentation

Current documentation describes the checked-out implementation. Last reviewed: **2026-09-26**.
Source review, a passing build, local integration tests and production acceptance are different checks; a feature documented here is not a claim that its deployed service has been verified.

## Start here

| Task | Read |
|---|---|
| Find a module or decide where a file belongs | [File structure](architecture/file-structure.md) |
| Understand clients, host, cloud and data ownership | [Architecture](architecture/overview.md) |
| Work on the macOS, Linux and Windows desktop app (Electron) | [Desktop app](architecture/desktop-app.md) |
| Build, install, restart and run checks | [Development](guides/development.md) |
| Test a phone over Cloudflare instead of LAN | [Cloudflare testing](guides/cloudflare-testing.md) |
| Configure dev/main or deploy the services | [Environments and deployment](guides/environments-and-deployment.md) |
| Run the host on a Linux server or cloud VM | [Linux servers](guides/linux-servers.md) |
| Reproduce or check something on Windows (UTM VM) | [Windows testing](guides/windows-testing.md) |
| Set up accounts, approvals or desktop SSH | [Accounts and SSH](guides/accounts-and-ssh.md) |
| Change transport or cryptography | [Remote protocol](reference/remote-relay.md) and [shared vectors](reference/fixtures/README.md) |
| Change what the host and clients exchange | [Client and host compatibility](reference/compatibility.md) |
| Change an iPhone or desktop screen or button | [UI conventions](design/ui-conventions.md) |
| Write or refactor code in any module | [Clean code](guides/clean-code.md) |

## Features

- [Group chats and reply threads](features/groups-and-threads.md)
- [Bot-to-bot collaboration](features/bot-collaboration.md)
- [Context and memory](features/context-and-memory.md)
- [Bot workspaces](features/bot-workspaces.md)
- [Voice calls](features/voice-call.md)
- [File attachments](features/file-attachments.md)
- [Remote screen](features/remote-screen.md)
- [Notch island (desktop)](features/notch-island.md)
- [Extensions and agent setup](features/marketplace.md)
- [Connector setup and credentials](features/connector-credentials.md)
- [Routines](features/routines.md)
- [Widgets, Live Activities and onboarding](design/mobile-widgets.md)
- [Notifications and background Live Activity updates](design/push-and-live-activity.md)

## Documentation layout

```text
docs/
├── README.md                  # Navigation and maintenance rules
├── architecture/              # Current boundaries, data flow and file ownership
├── guides/                    # Build, configuration, testing and deployment steps
├── reference/                 # Wire contracts and shared executable fixtures
│   └── fixtures/
├── features/                  # Current behavior and implementation entry points
├── design/                    # UI rules, rendering and system integration
└── archive/                   # Dated plans/audits; never current instructions
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
