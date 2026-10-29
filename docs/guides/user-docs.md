# User manual (codync.dev/docs)

The website has a user manual for people who use Codync: what a bot is, how to set one up, what it can do.
It lives in the site (`web/`): one MDX file per page in `web/content/docs/<slug>.mdx`, the sidebar order and each page's title and
description in `web/app/docs/pages.ts`, the pages' typography in `web/mdx-components.tsx`. The docs in this folder stay the
contributor's reference; the manual says the same things for users.

## Keep it current

A change a user can see or do differently updates the manual page that covers it, in the same change, like any
other doc it makes stale: a new or renamed setting, button, command or flow, a new agent, a changed limit or
number, a new privacy-relevant data flow. Add a page (file plus its `pages.ts` entry) when a feature fits none.
The site deploys `main` to codync.dev and `dev` to a preview, so the manual goes live with the release that ships
the change.

## Pages and their sources

| Page | Covers | Sources |
| --- | --- | --- |
| `getting-started` | Install, pair, first bot | `README.md`, [updates](updates.md), [dev app](dev-app.md) for names only |
| `bots` | Creating, settings, workspaces, hiding, permissions, new session | [bot workspaces](../features/bot-workspaces.md), [conversation runtime](../architecture/conversation-runtime.md) |
| `agents` | Known agents, registry, custom command, Extensions, sign-in, models | [extensions](../features/marketplace.md), `host/src/agent/backends.rs` |
| `conversations` | Messages, files, approval cards, stop, replies, reactions, commands, fresh conversations, notifications | [chat messages](../features/chat-messages.md), [file attachments](../features/file-attachments.md), [context and memory](../features/context-and-memory.md), [notifications](../design/push-and-live-activity.md) |
| `groups` | Group chats | [groups and replies](../features/groups-and-replies.md) |
| `teamwork` | Bots messaging bots, bots on your other computers, Manage the team | [bot collaboration](../features/bot-collaboration.md), [codync-network](../features/codync-network.md) |
| `cloud-assistants` | The MCP connector for ChatGPT, Claude and other assistants: turning it on, adding it, what it can do, privacy | [cloud assistants](../features/cloud-assistants.md) |
| `routines` | Schedules, webhooks, runs | [routines](../features/routines.md) |
| `artifacts` | Artifacts, the Library | [artifacts](../features/artifacts.md) |
| `memory` | Layers, dreaming, the agent's own memory | [context and memory](../features/context-and-memory.md), [bot memory](../design/bot-memory.md) |
| `connectors-and-skills` | MCP connectors, apps, skills | [connector credentials](../features/connector-credentials.md), [extensions](../features/marketplace.md) |
| `agent-fallbacks` | The fallback chain | [agent fallbacks](../features/agent-fallback.md) |
| `computer-use` | Remote screen, computer use | [remote screen](../features/remote-screen.md), [computer use](../features/computer-use.md) |
| `voice` | Calls, dictation, engines and keys | [voice calls](../features/voice-call.md) |
| `remote-access` | Relay, direct routes, pairing, accounts, reverse proxy, SSH computers | [remote protocol](../reference/remote-relay.md), [accounts and SSH](accounts-and-ssh.md) |
| `linux-servers` | Headless install, reaching it, firewall, approvals | [Linux servers](linux-servers.md) |
| `updates` | App and host updates, version mismatch, uninstall | [updates](updates.md), [compatibility](../reference/compatibility.md) |
| `privacy` | What stays on the computer, what Codync's services see, analytics | [analytics](../features/analytics.md), [remote protocol](../reference/remote-relay.md); keep it consistent with the policy page `web/app/privacy/page.tsx` |

## Writing rules

- English only. Plain, short sentences in the second person. Name things the way the apps show them (the bot's
  settings, **Manage the team**), with the real English UI labels in bold; no source paths, type or method names
  unless the reader types them (a CLI command, an environment variable).
- Where the apps differ, say how in each, briefly: **iPhone**, **Desktop**, **Terminal UI**.
- Describe the current release only. Leave out the Android app until it's public, Codync 1.x, products the UI
  was modeled on, multiple computers per account, roadmap and internal investigations.
- Every claim is checkable against the code or a doc above. Numbers and limits are written as the code has them.
  Security guidance (what the relay sees, loopback-only API, reverse-proxy rules) is exact.
- MDX: the page's title (its `#` heading) and description live in `pages.ts`, so the file holds only `##` and
  `###` sections, lists and small tables, no images or components. Never a bare `<`, `{` or `}` in prose: put code,
  paths and placeholders in backticks. Link pages as `/docs/<slug>`, sections as `/docs/<slug>#<heading-id>`.
- Check: in `web/`, `npm run check:docs` (also run by the pre-commit hook) compiles every page and checks that
  every `/docs` link and `#anchor` lands on an existing page and heading; `npm run build` must pass too.
