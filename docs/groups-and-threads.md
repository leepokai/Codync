# Group chats and threads

Both features live in the host. Every client (iPhone, Mac, Linux, terminal) calls the same
methods and renders the same data: none of them routes messages, parses mentions or counts
replies itself.

## Words

- **Chat**: a bot's (or group's) one endless conversation. Entries with `threadId == null`.
- **Thread**: replies on one main-chat message (the *root*), Slack's "reply in thread".
  Entries with `threadId == <root entry id>`. Threads are flat: a reply can't start a thread.
- **Group**: a roster row (`kind: "group"`) holding several bots and the user. It has a
  transcript but no agent, folder or harness of its own.
- **Lane**: where a turn talks, `(chat, thread)`. `host/src/store.rs` `Lane`.

## Data (all over the existing sync: `bot` and `entry` events)

- Bot: `kind` (`agent` | `group`), `members` (group: bot ids, max 6), `workingChat` /
  `workingThread` (where the running turn talks). A group's `status` / `activity` are its busy
  member's (`"Alice: Writing a reply…"`); its `lastMessage` names who spoke (`"Alice: …"`).
- Entry: `threadId`; `data.author` (the bot that wrote it; every agent-made entry has one);
  on a root, `data.thread = {count, lastAt, authors}` (`authors`: bot ids and `"user"`, first
  reply first), kept up to date by the host.
- `sync` / `events` catch-up carries recent entries of chats *and* threads. Paging:
  `history` (main chat only), `thread` (a whole thread).

## Methods (`POST /api/<method>`, same on the E2E channel)

| Method | Body | Notes |
|---|---|---|
| `send` | `botId, text, clientNonce, threadId?` | `botId` is a bot or a group. `threadId` replies in that root's thread. Works in the relay mailbox too. |
| `thread` | `botId, rootId` | `{entries}`: the thread's replies, oldest first. |
| `createBot` | `{kind:"group", name, members}` | Returns the existing group when these exact bots already share one. |
| `updateBot` | `{id, name?, members?, pinned?, hidden?}` | A group can't become a bot or the reverse. |
| `deleteBot` | `botId` | A group's bots stay. A deleted bot leaves its groups. |
| `stop` | `botId` | For a group: ends its room turns (main and threads) and stops members working in it. |
| `respondPermission` | `entryId, optionId?` | Routed to `data.author`, so cards in a group reach the bot that asked. |
| `markRead` | `botId` | Unread counts include thread replies. |

## How a group answers (`host/src/group.rs`, Grok Bot's design)

- The user's message starts a *room turn* in its lane: up to 3 rounds, 10 replies in all.
- Who answers each round: the members @-mentioned since the user's last message (full
  name, name without spaces, or first word; `@all` / `@everyone`), or everyone if none is.
  Members speak one at a time; each round starts one member later.
- A member runs the turn in its **own main session** (its history is unified across chats),
  told who's in the room and what was said since it last spoke there, framed as
  `[Group chat: "<name>" - with …]`. Its final reply is the one message the room sees; replying
  `(pass)` says nothing. A round where nobody speaks ends the room turn.
- A new message in the same lane, or Stop, ends a running room turn before its next speaker.
- Tool calls, thoughts and permission cards of a member's group turn are written to the
  group (with `author`), not to the member's own chat. One "done" push per room turn.

## How a thread continues (`host/src/bot.rs`)

- In a bot's chat, a thread is its own ACP session. The first reply forks the chat's session
  (`session/fork`, e.g. Claude) so the thread starts from everything said so far; an agent
  that can't fork starts a fresh session given the root and the lines before it. Nothing said
  in the thread reaches the main chat's session.
- In a group, a thread is its own room: the room turn runs in the thread's lane and each
  member is shown the root and the thread's messages.

## Client checklist

Roster row for groups (member avatars), author label on group replies, the thread summary
under a root (opens the thread), "Reply in thread" on any main-chat message, a thread view
(root, replies, composer sending `threadId`), group create/edit (name, members), Stop in a
group calls `stop` with the group id.
