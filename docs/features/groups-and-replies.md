# Group chats and replies

Both features live in the host. Every client (iPhone, Android, desktop, terminal) calls the same
methods and renders the same data: none of them routes messages, parses mentions or builds
quotes itself.

## Words

- **Chat**: a bot's (or group's) one endless conversation.
- **Reply**: a user message that answers an earlier message of the same chat (the user's own
  or a bot's final reply). It stays in the chat, in time order, and carries a quote of the
  message it answers.
- **Group**: a roster row (`kind: "group"`) holding several bots and the user. It has a
  transcript but no agent, folder or harness of its own.
- **Lane**: where a turn talks: a bot's own chat or a group's. `host/src/store/model.rs` `Lane`.

## Data (all over the existing sync: `bot` and `entry` events)

- Bot: `kind` (`agent` | `group`), `members` (group: one or more distinct agent bot IDs on the
  same computer; no nested groups), `workingChat` (where the running turn talks). A group's
  `status` / `activity` are its busy member's (`"Alice: Writing a reply…"`); its `lastMessage`
  names who spoke (`"Alice: …"`).
- Entry: `data.author` (the bot that wrote it; every agent-made entry has one); on a reply,
  `data.replyTo = {id, author, text}`: the answered message, who wrote it (`"user"` or a bot
  id) and its text (or its files' names when it has none), at most 500 bytes. The host builds
  it from `send`'s `replyTo` (`host/src/chat/replies.rs`).
- Read state: one per chat (`read_rev`). A bot's `unread` badge counts final replies and pending
  cards newer than it; a reacted-to message doesn't count again.
- The `events` catch-up carries each chat's recent entries; `history` pages older ones.

## Methods (`POST /api/<method>`, same on the E2E channel)

| Method | Body | Notes |
|---|---|---|
| `send` | `botId, text, clientNonce, replyTo?` | `botId` is a bot or a group. `replyTo`: the id of a user message or final reply in that chat; anything else is refused. Works in the relay mailbox too. |
| `createBot` | `{kind:"group", name, description?, members}` | Returns the existing group when these exact bots already share one. |
| `updateBot` | `{id, name?, members?, pinned?, hidden?, section?}` | A group can't become a bot or the reverse. `section`: a name, or `null` for none. |
| `deleteBot` | `botId` | A group's bots stay. A deleted bot leaves its groups. |
| `reorderBots` | `ids` | The roster as the user dragged it, top first; groups count as rows. See [bot order](computer-order.md#bot-order). |
| `renameSection` | `from, to` | Every row in section `from` moves to `to`, groups too; an existing name merges them, `null` removes the section. See [bot order](computer-order.md#bot-order). |
| `stop` | `botId` | For a group: ends its room turn and stops members working in it. |
| `react` | `entryId, emoji` | Toggles the user's reaction on a chat message (`data.reactions`, oldest first, at most 8). A reacted-to message never turns unread again. The bot isn't told, except in a group, where members see reactions on the room lines they're shown. Bots react themselves with the `chat` server's `react` ([bot reactions](chat-messages.md#bot-reactions)). |
| `respondPermission` | `entryId, optionId?` | Routed to `data.author`, so cards in a group reach the bot that asked. |
| `markRead` | `botId` | Reads the chat. A no-op when nothing there is unread, so clients call it whenever a chat is on screen. `send` marks its chat read. |

## How a group answers (`host/src/chat/group.rs`, Grok Bot's design)

- The user's message starts a *room turn* in the group: up to 3 rounds, 10 replies in all.
- Who answers the first round: the members the user @-mentioned (full name, name without
  spaces, or first word; `@all` / `@everyone`) or replied to, or everyone if none is. A reply to
  a member's message counts like `@`-mentioning it.
- Who answers a later round: only the members another member @-mentioned in this room turn
  (a member's `@all` / `@everyone`: all the others) and who haven't spoken or been asked since
  (one that passed after seeing the mention isn't asked again). Nobody named,
  and the room turn ends after the round: a member brings a teammate in by naming it, and a
  `(pass)` costs no extra rounds. Members speak one at a time; each round starts one member later.
- `message_bot` is refused in a room turn (its reply would go to the member's own chat); members
  @-mention each other instead ([bot collaboration](bot-collaboration.md)).
- A group's `description` ("About") tells its members what the room is for.
- A member runs the turn in its **own main session** (its history is unified across chats),
  told who's in the room and what was said since it last spoke there, framed as
  `[Group chat: "<name>" - with …]`. Its final reply is the one message the room sees; replying
  `(pass)` says nothing. A round where nobody speaks ends the room turn. A member can also
  react to someone's message (`react(emoji, to?)`), so it can agree with a reaction and pass.
  Room lines carry their reactions: `Ann: Shipped. (reactions: User 👍, Ben 🎉)`, and a reply
  its quote: `User: [Codync: this replies to Ann's message: "…"] text`.
- A member busy with a turn outside the room isn't waited for: the room shows "<name> is busy
  in another chat and will answer here when it's done.", moves on, and the member answers in
  the room once its other turn ends (with its own "done" push), on a prompt built then, so it
  sees what the room said meanwhile. It isn't asked again in later rounds of that room turn.
  Newer messages in the group, and Stop in the member's own chat, don't drop that late answer;
  the group's Stop or removing the member from the group does, and a newer room turn that asks
  the member again replaces it.
- A new message in the group, or Stop, ends a running room turn before its next speaker.
- Tool calls, thoughts and permission cards of a member's group turn are written to the
  group (with `author`), not to the member's own chat. One "done" push per room turn, plus
  one per late answer from a busy member.
- Files sent to a group are kept in the group's data folder (`~/.codync/bots/<group>/uploads`);
  each member's prompt lists their paths under the user's line.
- Voice calls work in a group: each final reply is read with its speaker's name, a member's
  pending permission card is announced (a group has no status of its own), and the realtime
  operator is told the room's members.

## How a reply reaches the agent (`host/src/chat/prompt.rs` `of_entry`)

In a bot's own chat a reply is an ordinary message in the same session. A note before its text
says which message it answers: `[Codync: this replies to your message: "…"]`, `… to the user's
earlier message: "…"]` or `… to Ann's message: "…"]`. It speaks as Codync, so the agent trusts the
quote even when the quoted message is no longer in its session. Memory never takes the
quote as the user's words: it stays in `data.replyTo`, apart from `data.text`.

## Client checklist

Roster row for groups (member avatars), a trace per turn, author label on group replies,
**Reply** on user messages and final replies (desktop: the hover footer beside the time, with
quick reactions and copy, never on the bubble, and the context menu; iPhone and Android:
long-press, reactions on top of the menu; terminal: `r` picks a message, ↵ or `r` replies, esc
cancels). While replying, the composer shows a quote
bar on top (`text.bubble`, “text” on one line, an X to cancel; the terminal: `↳ “…”`), its
placeholder reads "Reply…", and sending includes `replyTo`; leaving the chat cancels it. A
user message with `data.replyTo` shows the quote at the top of its bubble. Every client shows a
quote as its first non-empty line without Markdown markers (the roster's message preview rule). Reactions under
a message (the user's: tap to take back; the bots': emoji plus the avatars of who chose it),
group create/edit (name, members), Stop in a group calls `stop` with the group id.

Slack-style reply threads were removed for quote replies; [threads for teams](../plans/threads-for-teams.md)
lists where their code lived and how to bring it back.
