# Discord channel

The host reaches Discord as a bot: it opens the Gateway websocket itself, receives the messages of
bound channels and answers them in the same channel. Nothing has to be reachable from the internet,
so there is no endpoint, signing secret or tunnel to set up.

## Local owner setup

1. In the Discord developer portal, create an application with a Bot. Copy the bot token and the
   application's bot user id, and the id of the server (right-click the server → Copy Server ID).
2. In **Bot**, turn on **Message Content Intent** (privileged; without it the host sees no text).
3. Invite the bot to the server with the OAuth2 URL that includes `bot` and the Send Messages,
   Read Message History and Add Reactions scopes.
4. In **Settings → Channels** on the computer, paste the server id, bot user id and token, then add
   one row per channel: channel id, the bot that answers it, and the Discord user ids allowed to
   write there. The host checks the token with `/users/@me` and every channel with `/channels/{id}`
   before saving.

The token goes to the encrypted host vault; the status screen never shows it. A direct message from
an allowed person is answered too, with the first bot that allows them.

## Routing and durability

A message is accepted when it mentions the bot, when the channel already has a conversation with
it, or when it is a direct message from an allowed person. Everything else is dropped silently, so
the bot can sit in a busy server.

Accepted messages go through `discord_inbox` and `discord_threads` (one conversation per channel,
keeping the message the next reply answers) and prompts are claimed by the bot queue exactly like
Slack's. Replies are written into `discord_outbox` before the REST call, in pieces under Discord's
2000-character limit, and the first piece `reply_to`s the message that started the turn. A missing
channel permission or a deleted channel ends in `failed`, which the status endpoint lists with the
last 50 deliveries.

A message that matches a routine's `message.mention` or `message.created` trigger also starts that
routine with `{source:"discord", event, channel, user, text, url}`.

Authenticated APIs: `setDiscordChannel`, `disconnectDiscord` (local only) and `discordStatus`
(any paired client). Replacing the configuration starts a new generation and cancels older queued
prompts.

## Limits worth knowing

- The Gateway connection is re-`IDENTIFY`ed after a drop (no `RESUME`): a short drop can miss the
  messages sent while it lasted. Durability starts once a message is queued.
- The host asks for `GUILDS`, `GUILD_MESSAGES`, `DIRECT_MESSAGES` and `MESSAGE_CONTENT` only.
- Threads inside Discord are ordinary channels for this integration: bind the thread id to answer
  in it.
- Discord's own bot is not a Codync bot; the assigned Codync bot keeps its own session, memory and
  approvals.
