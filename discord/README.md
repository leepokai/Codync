# Codync Discord

Cloudflare Worker at `https://discord.codync.dev`: one link that adds a member to the Codync Discord and gives
the GitHub roles.

1. `/` sends the member to Discord sign-in (`identify connections guilds.join`).
2. `/callback` adds them to the server, then takes the GitHub account connected in their Discord profile
   (verified by Discord). Without one, they sign in with GitHub (`/github/callback`); the Discord id travels in a
   signed cookie, so nobody can link a GitHub account to someone else's Discord.
3. A merged pull request on `leepokai/Codync` earns **Contributor**, an opened issue **Reporter**; the bot adds
   each role earned. The link is stored in KV (`github:<GitHub user id>` → Discord user id), and each new link
   or role is posted to #moderator-only (no pings).
4. `/webhook` takes the repo's `pull_request` and `issues` events: a linked member gets the role the moment
   their pull request merges or their issue opens.
5. `/mentions` takes release notes signed with `MENTIONS_SECRET` and turns each `@login` of a linked member into
   a Discord mention. Auto Tag (`.github/workflows/auto-tag.yml`) uses it so the #releases post pings the people
   it thanks; the links stay private.
6. Milestones go to #milestones, each once and without pinging anyone, with progress toward the goal and one-click
   links to share Codync on X, Threads, Reddit and LinkedIn: every 100th star (`/webhook` takes the repo's
   `star` events; goal `STAR_GOAL`) and every 1,000th desktop app download (GitHub has no download event, so a
   daily cron sums the installer downloads of every release: `.dmg`, `.exe`, `.AppImage`, `.deb`, `.rpm`,
   desktop Linux `.tar.gz`; goal `DOWNLOAD_GOAL`). The last milestone posted is kept in KV (`stars:milestone`,
   `downloads:milestone`), so a count that dips and climbs back doesn't repeat it.
7. Every GitHub issue gets its own post, tagged **GitHub**, in #feature-requests (the `enhancement` label or a
   "[Feature]" title) or #bug-reports (everything else), from the `issues` webhook. Closing on GitHub posts in
   the thread and closes it (with **Fixed** / **Shipped** when completed); reopening reopens it. Replies in
   Discord stay in Discord. KV keeps `issue:<number>` → `{ kind, thread }`, and the daily cron posts any open
   issue that has no post yet (that brought the existing ones over, and it catches a missed webhook).

`.github/workflows/contributor.yml` comments the link on a contributor's first merged pull request and first
issue. The onboarding Server Guide has a "Link your GitHub" to-do that leads to the link in #welcome.

## Setup

- Discord application **Codync** (Developer Portal): OAuth2 redirect `https://discord.codync.dev/callback`; bot
  not public. In the server, its role keeps Manage Roles and Create Invite (`guilds.join` needs it), sits above
  Contributor and Reporter, and can view and send in #moderator-only and #milestones (read-only for everyone
  else). On the Support category it may create posts, send in threads and
  manage threads (the moderator-only **GitHub** and **Shipped** tags, closing posts).
- GitHub OAuth app **Codync Discord** (leepokai's developer settings): callback
  `https://discord.codync.dev/github/callback`, no scopes requested.
- Repo webhook: `https://discord.codync.dev/webhook`, JSON, events `pull_request`, `issues` and `star`, secret =
  `GITHUB_WEBHOOK_SECRET`.
- `wrangler.toml` holds the public values (client ids, guild, role and channel ids, milestone goals, repo), the
  daily cron and the `LINKS` KV namespace.

```bash
npm ci
npx wrangler secret put DISCORD_CLIENT_SECRET
npx wrangler secret put DISCORD_BOT_TOKEN
npx wrangler secret put GITHUB_CLIENT_SECRET   # the OAuth app's
npx wrangler secret put GITHUB_TOKEN           # classic token, no scopes (public read only)
npx wrangler secret put GITHUB_WEBHOOK_SECRET  # the repo webhook's
npx wrangler secret put SESSION_SECRET         # any random string (openssl rand -hex 32)
npx wrangler secret put MENTIONS_SECRET        # same value as the repo secret DISCORD_MENTIONS_SECRET
npm run deploy
```

## Test

```bash
npm run typecheck
npm test
```
