# Chat messages

How a bot's work turns into what the user reads, copied from Grok Bot.

## Markdown links

Markdown links in bot replies are underlined on desktop and iPhone so they remain
recognizable in both light and dark themes. Hovering a desktop link shows its
destination URL; clicking opens it in the default browser. iPhone links keep the
system link interaction. The terminal already underlines Markdown link labels;
it has no pointer tooltip or link-opening action.

## Messages are sent, not streamed

- Every bot gets the built-in `chat` MCP server (`codync-host mcp chat`,
  `host/src/chat/outbox.rs`) with `send_message {text}` and `send_file {path, name?}`. In the bot's own chat, each call is one chat bubble, added to the turn's lane at once as an `agent` entry with
  `final: true`. A turn can send several.
- The agent's own reply text (`agent_message_chunk`) stays in the trace when the turn sent
  text through `send_message`. A turn that sent nothing (an agent that ignores the tool, or has no MCP) falls back to
  the old rule: its last text becomes the turn's final message, unless that text is `(pass)`: a message that
  needs neither words nor a reaction gets nothing, as in a group.
- File cards are separate final entries. Sharing a file preserves an intended final text
  reply; a files-only turn uses its filename for the completion summary. See
  [file attachments](file-attachments.md#files-from-a-bot). Artifact cards (`publish_artifact`) are final entries the
  same way; see [artifacts](artifacts.md).
- Group room turns, bot requests (`message_bot`) and routines reject `send_message` and `send_file`: their reply
  is the turn's last text, as before (`chat/group.rs`, `chat/team.rs`, `routines`).
- Between turns, work the agent goes on with in the background (a subagent's result) can still `send_message`
  (`send_between_turns`): a final entry in the bot's main lane, numbered with its latest turn, plus a *done* push;
  refused when the user stopped the last turn. `send_file` and `publish_artifact` need a running turn, and say to
  use `send_message` instead. The host can't tell a background call from the running turn's own (the `chat` MCP
  call names no session), so one that lands during a room, request or routine turn is refused like that turn's
  and told to send again once it ends.
- A bot request's reply stays in the recipient's trace and the request notice; it does
  not also become a message to the user. It goes back to the requesting bot as a turn of its own in
  that bot's main chat, and that turn's reply is an ordinary message to the user.
- The rules reach the agent twice: the MCP server's instructions, and the frozen instruction
  snapshot (`chat/context.rs`).
- The same server has `set_name {name}` and `set_instructions {instructions}` (`host/src/chat/profile.rs`): the
  bot changes its own name or instructions (its description) when the user asks it to, exactly as an edit in its
  settings would, and a notice ("Renamed to …", "Instructions updated") shows it in the main chat. A name set this
  way ends automatic naming. Its `save_skill` writes a skill for the bot, which the user can turn on for others
  ([skills bots write](marketplace.md)).
- A bot-to-bot exchange (`message_bot`) is one compact notice row, "Messaged Owen" /
  "Message from Egan". Its text lives only in the bot conversation sheet it opens; follow-ups
  are normal bubbles. Consecutive exchanges with the same peer in the main chat (either
  direction, trace entries ignored) collapse into one "3 messages with Owen" row ([bot collaboration](bot-collaboration.md#notice-data)).
- Clients never stream text into a bubble: the chat shows user messages, `final` agent entries,
  permission cards and notices (`isChat`). Nothing pops in and out while a turn runs.

## Bot reactions

- The `chat` server's `react {emoji, to?}` (`host/src/chat/reactions.rs`) goes through the bot's actor,
  which puts one emoji on the message the turn answers: in its own chat, the user's message that
  started the turn; in a group, the newest message its prompt showed from `to` ("user" or a participant's
  name), or from anyone else. Routines and bot requests can't react. Each bot keeps one reaction per
  message; a new emoji replaces it.
- Stored as `data.botReactions: [{bot, emoji}]` beside the user's own `data.reactions`. The first one also
  records `data.contentRev`, the rev before it, which the unread count uses, so a read message stays read.
- Reactions are rare. The instructions allow them only where a person would react (thanks, a plain OK, big
  news, a real joke), never on a question or a request, and the host refuses a bot's reaction while it
  reacted to another of the chat's last 6 messages (`reacted_lately`); a turn whose only answer was that
  refused reaction says nothing and sends no push. A reaction can be the whole reply: a turn that reacted and sent no message
  doesn't fall back to its last text, and its push says "Reacted ❤️ to your message".
- Clients show each bot emoji as a chip with the avatars of the bots that chose it (up to three), after the
  user's own chips; it isn't tappable, and a bot that was deleted drops out. The terminal UI lists
  `❤️ Ann, Ben` after the user's reactions.

## While it works

The working line under the last message shows the bot's activity; composing a `send_message`
reads as **Typing…** (`activity()` in `agent/bot/tool_entry.rs`). The activity indicator shows an
animated orb, a short status and elapsed time, in a pill no wider than an agent bubble whose status
truncates to one line. Replies arrive as complete messages.

**Show thinking details** (Settings › Chat; each device's own choice, off by default) decides whether the pill
unfolds; off, it stays a plain status line. On, the pill ends in a chevron, and a tap (click) unfolds the running
turn's steps inside it:
the turn's thinking text, narration (never a `(pass)`), tool calls (MCP tools included) and plan, in order, with each
tool's output and diffs one more tap away. The pill widens to an agent bubble's width, the steps
scroll inside it past 280 pt and follow new ones until the reader scrolls up, and each turn starts
folded; the pill leaves with the turn. The host sets `bookkeeping` on every tool entry: `true` for
the bot's own bookkeeping (a `send_message` call, a `memory` MCP call, or a call naming a file in a
memory folder the bot sees), which the steps leave out, as the working line does. A host older than
2.25.0 sets none, so its tool calls stay out of the steps too. A turn with no steps yet unfolds to
**Nothing yet.**: some agents send no thinking text, and Claude's arrives only as its thinking ends.
Claude Code leaves an SDK session's thinking text out unless asked, so Claude bots start with
summaries shown (`--thinking-display summarized`, through `claudeCode.options.extraArgs` in
`agent/bot/launch.rs`). Whether Claude thinks stays the user's Claude Code setting; when it does, a
short thought's summary arrives in one piece when the thinking ends, a long one's in parts.

Full conversation remains available through a message's Show what it did (its touch-and-hold
menu on the iPhone and Android, its actions on the desktop). It contains narration, all
thoughts, tools and previous turns, and follows new steps until the reader scrolls up.
On the terminal, with thinking details turned on (`^k` › Show thinking details, kept in
`tui-thinking-details`), `>` or a click on the working line unfolds the steps under it (their last
14 lines), `t` opens Full conversation, and the working line keeps the count of steps so far;
once the turn ends, its last message's `↳ N steps` line counts them the same way.

## Motion and scrolling

- A message arriving while the list follows the newest pops in: 0.24 s,
  `cubic-bezier(.23, 1, .32, 1)`, from 12 pt lower at 94 % scale (iPhone: `ConversationLayout` in
  `ConversationList.swift`; desktop: `.row-in` in `thread.css`; reduced motion: a 0.12 s fade).
- The list stays pinned to the bottom while the reader is there; scrolling up releases it and the
  round "Jump to latest" button brings it back. Sending always returns to the bottom.
- Opening or closing the phone keyboard keeps a bottom-following chat on its latest message;
  reading older messages preserves that position. iPhone viewport updates do not count as user
  scrolling and wait for any active touch to finish before applying a deferred bottom follow.
  Android already follows viewport/padding changes; desktop observes transcript resizing, and
  the terminal renders its bottom-relative scroll position without a software keyboard.
- iOS 26+ uses the native circular Liquid Glass button for "Jump to latest", with a material
  fallback on older iOS. Android and Electron retain their platform controls; neither exposes
  Apple's native Liquid Glass button API. The terminal keeps its keyboard action.
- iPhone: earlier messages load when the top of the chat comes into view, so a short chat never
  shows a lasting spinner. Pulling down past the top sweeps in the `arrow.clockwise` symbol; letting
  go of a full sweep (`PullToRefresh.swift`) reconnects to the computer while the chat waits just
  below the turning arrow, then eases back. A pull that isn't full refreshes nothing. The desktop
  reconnects from the computer menu instead.
- The terminal UI lists the same messages; its steps line goes under the turn's last message.

- Electron keeps the activity row mounted during its 0.42 s exit and animates its height,
  opacity and a small scale change together. The transition honors Reduce Motion and reverses
  when work restarts during an exit. iPhone conversation insertion, removal and scrolling
  keep their existing animations.
