# Chat messages

How a bot's work turns into what the user reads, copied from Grok Bot.

## Messages are sent, not streamed

- Every bot gets the built-in `chat` MCP server (`codync-host mcp chat`,
  `host/src/chat/outbox.rs`) with `send_message {text}` and `send_file {path, name?}`. In the bot's own chat and its
  threads, each call is one chat bubble, added to the turn's lane at once as an `agent` entry with
  `final: true`. A turn can send several.
- The agent's own reply text (`agent_message_chunk`) stays in the trace when the turn sent
  text through `send_message`. A turn that sent nothing (an agent that ignores the tool, or has no MCP) falls back to
  the old rule: its last text becomes the turn's final message.
- File cards are separate final entries. Sharing a file preserves an intended final text
  reply; a files-only turn uses its filename for the completion summary. See
  [file attachments](file-attachments.md#files-from-a-bot).
- Group room turns, `ask_bot` requests and routines reject `send_message` and `send_file`: their reply is the
  turn's last text, as before (`chat/group.rs`, `chat/team.rs`, `routines`).
- An `ask_bot` reply stays in the recipient's trace and the request notice; it does
  not also become a message to the user. The requesting bot uses it to answer the user.
  Independent `message_bot` turns report to the user in the recipient's main chat, through
  `send_message` or the final-text fallback, and do not add that report to the request notice.
- The rules reach the agent twice: the MCP server's instructions, and the frozen instruction
  snapshot (`chat/context.rs`).
- A bot-to-bot exchange (`ask_bot`, `message_bot`) is one compact notice row, "Messaged Owen" /
  "Message from Egan". Its text lives only in the bot conversation sheet it opens; follow-ups
  are normal bubbles. Consecutive exchanges with the same peer in the main chat (either
  direction, trace entries ignored) collapse into one "3 messages with Owen" row ([bot collaboration](bot-collaboration.md#notice-data)).
- Clients never stream text into a bubble: the chat shows user messages, `final` agent entries,
  permission cards and notices (`isChat`). Nothing pops in and out while a turn runs.

## While it works

The working line under the last message shows the bot's activity; composing a `send_message`
reads as **Typing…** (`activity()` in `agent/bot.rs`). Thoughts and tool calls stay in the
Full conversation sheet.

## Motion and scrolling

- A message arriving while the list follows the newest pops in: 0.24 s,
  `cubic-bezier(.23, 1, .32, 1)`, from 12 pt lower at 94 % scale (iPhone: `ConversationLayout` in
  `ConversationList.swift`; desktop: `.row-in` in `thread.css`; reduced motion: a 0.12 s fade).
- The list stays pinned to the bottom while the reader is there; scrolling up releases it and the
  round "Jump to latest" button brings it back. Sending always returns to the bottom.
- iPhone: earlier messages load when the top of the chat comes into view, so a short chat never
  shows a lasting spinner. Pulling down past the top sweeps in the `arrow.clockwise` symbol; letting
  go of a full sweep (`PullToRefresh.swift`) reconnects to the computer while the chat waits just
  below the turning arrow, then eases back. A pull that isn't full refreshes nothing. The desktop
  reconnects from the computer menu instead.
- The terminal UI lists the same messages; its steps line goes under the turn's last message.
