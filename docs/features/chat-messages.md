# Chat messages

How a bot's work turns into what the user reads, copied from Grok Bot.

## Choice cards

When the user should pick among a few ways forward, a bot sends a card instead of listing them: `request_decision {question, options, recommended?}` on its `chat` server (two to twelve different options). The card shows the question, a numbered button per option (the suggested one marked) and a last row for the user's own words (marked with a pencil, since a typed number picks an option), in the bot's chat on the phone, the computer and the terminal UI (where the message box takes the own words); clients that don't draw the card yet still show its text (the question, the options, and the suggestion).

The card is a `notice` entry with optional additive data: `decision: true`, `question`, `choices` (strings), `recommended` (an index or null), `status` `pending`/`answered`/`expired`, and `selected`. `text` always holds the full question, numbered choices and suggestion. It does not reuse permission `options`, whose elements remain objects for older phone decoders. Older clients already render notices. Any plain typed message answers the waiting card: an exact option, its one-based number, or else the user's own words. Exact labels take priority, including numeric labels. Messages with attachments or a quote, `/` commands (skills and agent commands, which must reach the agent) and speech from a call are ordinary messages and expire the waiting card; a call's own transcript can still name an option or its number. The card's own-words row sends an ordinary message, so it needs no host method and degrades to a plain message on an older host.

The bot shows as needing the user and sends the usual response-needed notification. The call waits up to 270 seconds (Codex drops an MCP call at 300 s, [codex-acp](../upstream-agents/codex-acp.md)), with a 290-second budget through the chat MCP HTTP bridge. When that runs out the call returns `{answered: false, note}` and the card stays `pending` as the actor's open card (one per chat, in memory): a later press marks it answered and posts the choice as the user's next message (`from: decision`, `decisionId`, `replyTo` quoting the question), a new turn; the user's next message answers it under the typed rules above (it goes to the bot as their next message, with `decisionId`, `decisionChoice` and the same `replyTo`, plus a `[Selected: …]` line when they typed an option or its number), or expires it when it's something else. A press in the last second before the deadline already counts as late, so it never races the call's own timer. A new card or a host restart expires it; a task's card still expires at the deadline (the call gets an error), since a press there has no next message to become. A live card expires when its turn ends, the user stops, the agent cancels its call, or the host restarts. Successful answers and expiry clear the waiting state unless a permission card still waits.

MCP clients can cancel a pending call with `notifications/cancelled` and its exact JSON-RPC `requestId` on the same server process. The bridge aborts that request; closing the HTTP wait drops the choice receiver, which the actor's periodic cleanup expires. Unknown, completed, or differently typed IDs are ignored. Closing the MCP process's stdin ends that process and its pending requests. Neither path is a command to stop the bot or choose a default. Cancellation that arrives after an answer was delivered does not undo it. `decision_cancellation_e2e` checks these transport and card-state expectations through actual stdio and HTTP, separately from the 270-second deadline tests.

A press (`respondDecision {entryId, option}`, the option's full text) answers the live call with `{choice}`; the user's own words answer it with `{choice, custom: true}`. The actor records exactly one user message in the same turn (`from: decision`); it never queues another turn (except for the open card above). Text replies reuse their existing user entry. API success requires delivery to the live waiter or the open card's message, so a stale card cannot claim success. Room turns, routines, bot requests and other injected turns ask in a message instead. The actor reports `decision_asked` after publication and `decision_answered` after delivery. Terminal keys F1–F12 reach all choices; keys 1–9 also select the first nine by position while the bot waits on the card (or the card is picked), and otherwise jump to a bot, so an open card never takes a jump key.

### An agent's own questions (ACP elicitation)

An agent can ask the user something mid-turn without the chat MCP server: the host advertises `clientCapabilities.elicitation: {form: {}}` in `initialize`, and an `elicitation/create` (ACP's unstable surface; form mode, for the running turn's session) shows on the same choice card (`host/src/agent/elicitation.rs`, `host/src/agent/bot/elicitations.rs`). Hermes sends its clarify questions this way when the capability is there, and otherwise as a permission request, which **Approve automatically** would answer with its first option. A question is never a permission: the bot's approval setting doesn't answer it, and nothing picks the form's `default`, which shows as the suggested option.

A form with exactly one field and a question of at most 2000 characters (as `request_decision` allows) maps onto the card: a string `enum` (or titled `oneOf`, whose titles show and whose `const` goes back) with 2 to 12 options becomes the options; a `boolean` becomes **Yes** / **No**; a plain string (no `pattern`, `format`, `maxLength` or `minLength` above 1) becomes a card with no options, answered by the user's own words (its own-words row or a plain typed message; every client draws such a card as the question and that row). A card with options takes only an option or its number (`ownWords: false` in its data, so every client hides its own-words row and points to no message box; a missing field keeps the row; its text says so for older clients): own words typed in the message box there are an ordinary message, which expires the card and reaches the bot as the user's next message. Number and integer fields, multi-select arrays, several fields, URL mode, and questions in a room's turn, a routine's run, another bot's request, a host-started turn or while another card waits are declined (`{action: "decline"}`) rather than shown unfaithfully; a thread's card takes presses only, so a thread's free-text question is declined too. Declining rather than a JSON-RPC error lets the agent go on (Hermes takes either reply as a skipped question; an error would leave its question waiting).

The card waits with no time limit: the request stays open until the user answers (`{action: "accept", content: {<field>: <value>}}`: the words, the option's value, or `true`/`false`), and is cancelled (`{action: "cancel"}`) when its card stops waiting unanswered: the user stops the turn or sends a message the card can't take, the turn ends, or the actor or host goes away (the card expires as above). One card waits at a time, so `request_decision` is refused while a question waits. Analytics: `decision_asked` with `asker: agent`, the card's usual `decision_answered` / `decision_expired`, and `elicitation_replied` with what the agent was told.

## Markdown links

Markdown links in bot replies are underlined on desktop and iPhone so they remain
recognizable in both light and dark themes. Hovering a desktop link shows its
destination URL; clicking opens it in the default browser. iPhone links keep the
system link interaction. The terminal already underlines Markdown link labels;
it has no pointer tooltip or link-opening action.

## Messages are sent, not streamed

- Every bot gets the built-in `chat` MCP server (`codync-host mcp chat`,
  `host/src/chat/outbox.rs`) with `send_message {text, reply_to?}` and `send_file {path, name?, text?, reply_to?}` (`text`: the message a voice note plays in, [file attachments](file-attachments.md#a-voice-note-inside-its-message); `reply_to`: a message the bubble quotes, [bots quote too](groups-and-replies.md#bots-quote-too)). In the bot's own chat, each call is one chat bubble, added to the turn's lane at once as an `agent` entry with
  `final: true`. A turn can send several.
- The agent's own reply text (`agent_message_chunk`) stays in the trace when the turn sent
  text through `send_message`. A turn that sent nothing (an agent that ignores the tool, or has no MCP) falls back to
  the old rule: its last text becomes the turn's final message, unless that text is `(pass)`: a message that
  needs neither words nor a reaction gets nothing, as in a group. In a turn a steered message joined, what the agent
  wrote after its last `send_message` or reaction becomes a final message too, since either may have answered only
  what came before that message.
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

## Threads in the chat

A [thread](work.md)'s own transcript never enters the chat. Three things of it do:

- **Its approval cards**: a permission entry in the bot's chat with `data.thread = {id, title}` (the title is its
  task's) and `data.task`, the task's id, shown with the line
  `Work · <title>` and answered like any card (`respondPermission` reaches the thread's actor).
- **Its report**: a host-started turn (`HostTurn::TaskReport`) whose reply is the bot's ordinary message. The prompt
  is `from: host`, shows no bubble, and isn't the user's words to memory.
- **A report the bot didn't answer** (its turn was stopped, failed or never started): an info notice,
  `"<title>" finished in the background: …` or `"<title>" did not finish: …`.

The capsule above the composer isn't an entry: it is the bot's `work`.

## Scheduled messages

*Schedule send…* holds a typed message for a later time: the composer's clock button (a draft, no
files, a 1:1 chat, the host online — an ordinary send still queues offline, the clock doesn't) opens a
local-time picker, and the message waits until then. The time must be at least 30 seconds out; a
nearer pick is refused — send the message now instead. A scheduled message carries the active reply
quote, exactly as a send. There is no second
queue and no second scheduler: a scheduled message is an ordinary user Entry whose `data.status` is
`scheduled` with a `notBefore` epoch-ms UTC (`chat/messages::save_scheduled`); a sweep
(`chat/schedule`, every 15 s) flips due entries to `queued` and hands them on through the ordinary
`hand_on` path, so batching, ordering behind live sends, turn numbering and bot-state handling are
the existing code's. The fire-time turn is allocated at fire (`max_turn + 1`), so chat order matches
delivery order; a restart keeps scheduled entries while expiring queued ones (`expire_pending` only
fails `queued`), and a host that was down delivers on its next start. Pause: a paused bot is never
woken (the pause gate lands with its own change; the sweep will hold due messages while paused).

The pending tray above the composer shows both kinds as one uniform thing: the live queue (while
the bot works) then the scheduled by fire time, each row with the clock as its single time control —
on a plain queued row it sets a time (the message leaves the live queue to fire then), on a
scheduled one it edits it, or clears it back to an ordinary send. Every row expands to its full text
on a click, copies its text, and edits its text in place (`updateScheduled` handles both kinds; a
waiting one is moved by its actor, as `steer` and `unqueue` are). *Send now* and *Remove* stay:
`sendScheduledNow` delivers through the ordinary queue, `cancelScheduled` never delivers. Hosts
before the feature advertise no `scheduleSend` capability, so clients hide the affordances.

A held message reaches the tray however far back its place in the chat is: the host's `hello` lists every
entry still `scheduled` in any chat (`hello.held`), and the tray shows what the chat's own loaded window
does not carry — the window holding the newest entries per bot, an older held message would otherwise be
missing from the one place it can be seen, edited or taken back, while the sweep still delivered it. An
entry the chat holds wins over the hello's copy, since an edit, a cancel or the fire itself lands there
first, and a message taken back is no longer held.

Analytics: `message_scheduled` (`scheduleSend`; `kind`, `agent`, `reply`),
`message_schedule_edited` (`what`: `time`/`text`), `scheduled_message_removed`,
`scheduled_message_sent_now`, and `message_schedule_failed` when one of those is refused
or a due message cannot be handed on (`stage`, `error`; never the message). The delivery
itself is the ordinary turn's `turn_finished`.

Phones are unaffected by design: their code keys off exact statuses, so a scheduled entry renders
as a plain bubble (no crash, no tray interference) and the affordance is desktop-only in v1.

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

A Claude Code bot's subagents show in the steps (the unfolded pill, Full conversation and a task's thread alike):
each subagent's tool calls sit indented under the call that launched it, and that call carries a quiet second line
with the subagent's type, the tokens it used and how long it ran once it finished
(`general-purpose · 42.3k tokens · 6 s`), or `in background` for one launched in the background, which reports no
totals. The host stores the facts on the tool entries: `parent` (the `toolCallId` of the launching call) on each of
the subagent's calls, and `subagent` (`kind`, `model`, `background`, `tokens`, `toolUses`, `durationMs`, each kept
until an update sends it again) on the launch; other calls carry neither. A step whose launch isn't among the steps
shown stays where it is. The facts come from Claude's adapter only
([claude-agent-acp](../upstream-agents/claude-agent-acp.md#subagents)); other agents' steps stay one flat list.

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
  `ConversationList.swift`; desktop: `.row-in` in `chat.css`; reduced motion: a 0.12 s fade).
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
- Android: the phone stores each chat on disk and holds only a window of it in memory: the newest
  300 messages of the open chat and the 5 opened last, the newest 30 rows of every other chat.
  Earlier messages load once the top of the chat is on screen: rows the phone already stored come
  first, then a page from the computer, and the window grows by 200 messages each time. It stops at
  3,000 messages or a per-chat memory budget (tool-heavy chats sooner); the iPhone pages to the
  first message. `apps/maestro/history.yaml` scrolls a long chat back to its first message on both phones.
- The terminal UI lists the same messages; its steps line goes under the turn's last message.

- Electron keeps the activity row mounted during its 0.42 s exit and animates its height,
  opacity and a small scale change together. The transition honors Reduce Motion and reverses
  when work restarts during an exit. iPhone conversation insertion, removal and scrolling
  keep their existing animations.
