# Slack: manual checklist with a real workspace

Nothing below has been run against real Slack. The host's tests use a fake Slack (`host/src/slack/socket_tests.rs`):
signed events, retries, duplicates, Socket Mode envelopes and reconnection. Real Slack is yours to check.

## Set up the Slack app (once)

1. Create an app at api.slack.com/apps (from scratch) in a test workspace.
2. **Socket Mode** → on. Create an app-level token with scope `connections:write` (`xapp-…`).
3. **Event Subscriptions** → on, subscribe to bot events `app_mention` and `message.im` (and `message.channels` /
   `message.groups` if the bot should follow thread replies).
4. **OAuth & Permissions** → bot scopes `app_mentions:read`, `chat:write`, `im:history` (`channels:history` /
   `groups:history` for thread replies). Install to the workspace and copy the bot token (`xoxb-…`).
5. Note the workspace id (`T…`), the bot's user id (`U…`), a channel id (`C…`) and your own user id (`U…`).

## Connect Codync

6. Call `setSlackChannel` (until the Channels screen exists) with `workspaceId`, `botUserId`, `botToken`, `appToken`,
   `signingSecret: ""`, and one binding `{channelId, botId, allowedUsers: ["<your U…>"]}`.
   With Socket Mode no public URL or signing secret is needed; `slackStatus` shows `socketMode: true`.
   (With HTTP instead, leave `appToken` empty, give the 32-hex signing secret and point Slack's request URL at
   `/hooks/slack`.)

## Check each of these

- [ ] Mention the bot in the bound channel: a Slack conversation appears under the bot and it answers in the thread.
- [ ] Reply in that thread without mentioning (needs the history scopes): the bot answers.
- [ ] Send the bot a direct message: it answers.
- [ ] Someone not in `allowedUsers` mentions the bot: nothing happens, no error shown to them.
- [ ] Mention the bot in a channel that is not bound: nothing happens.
- [ ] Kill the host process while Slack has an unacknowledged event, start it again: the event arrives once.
- [ ] Turn the network off for a minute and on again: the host reconnects by itself (log:
  "Slack Socket Mode lost its connection"), and new mentions arrive.
- [ ] Rotate the app token in Slack, then set the new one with `setSlackChannel`: the old socket closes and a new one
  opens; with the old token the log says "Slack refused the app token".
- [ ] `disconnectSlack`: mentions stop being answered; queued prompts are cancelled.
- [ ] A very long message (over 32 KB of text) and an empty mention are ignored.

Write what you see (and what differs) in the PR or in `slack-report.md` when you do this.
