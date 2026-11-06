# Context and memory

How a bot keeps its instructions, context and long-term memory. Codync owns memory; the ACP harness owns the
conversation and compaction. The design, including what is still to come, is in
[bot memory and compaction](../design/bot-memory.md).

## Transcript, context and memory

| | Where | Owner | Lifetime |
|---|---|---|---|
| Transcript | SQLite `entries` | Codync | One persistent chat per bot; replies quote the message they answer |
| Model context | ACP session (`bots.session_id`) | The harness | Until a rollover (Fresh conversations), a host-side `newSession`, a failed resume, or an agent/command/folder change (the apps offer no New session); each [fallback agent](agent-fallback.md) keeps its own, parked while another answers |
| Long-term memory | Markdown files per layer (`MEMORY.md` and notes); versions and who wrote what in SQLite `memory_*` tables | Codync: the bots, through their tools or their own file tools, and dreaming | Forever, across sessions |

Codync never replays the transcript into a session. Each turn sends only the new message(s); the harness keeps
its own context and compacts it itself.

## Instruction snapshot (`host/src/chat/context.rs`)

- The bot's instructions are rendered once, in English and kept general: profile (name, description), how to
  message the user, where the user is (Codync's apps, not the engine's own CLI; one conversation that goes on, so no starting over),
  other conversations, workspace, skills, plus the memory section. How to use each tool
  lives with the tool (MCP server instructions, tool descriptions), not here. They're stored as a snapshot in `kv` under `context.<bot>`, keyed by **session + compaction
  epoch**, and don't change until one of those changes. This keeps the prompt byte-identical so the prompt
  cache stays warm.
- How the snapshot reaches the agent (`Instructions` in `agent/bot`):
  - **Claude** (`agentCapabilities._meta.claudeCode` present) receives it as a real system prompt:
    `_meta.systemPrompt.append` on `session/new` and `session/load`. It survives compaction.
  - **An agent advertising ACP `sessionCapabilities.systemPrompt`** receives it as `systemPrompt`
    (`append`) on `session/new`, `session/load` and `session/fork`, and keeps it across compaction.
  - **Every other harness** receives it in the first message of each session (`<bot-profile>…</bot-profile>`; repeated with each message until one carrying it succeeds, the snapshot's `unprompted`, kept across a host restart),
    and once more, marked as replacing the earlier one, on the first message after a compaction.
- **Compaction epoch:** Codync advertises `clientCapabilities.session.compaction`. Each completed
  `compaction_update` (deduplicated by `compactionId`) increments `context.epoch.<bot>`; for an agent that has
  never sent one, so does its `usage_update.used` falling below half of a previous value of at least 20k. The
  next turn re-renders the snapshot, and Claude's session is loaded again with the new system prompt. The adapter
  rebuilds its query and resumes the same conversation.
- **Profile edits mid-session** (name, description, skills, the lead role) are sent once as an `<agent_profile_update>` block
  appended to the next message. After that turn reaches the agent, the change is recorded in
  `snapshot.announced`. The next compaction folds it into the snapshot.

## Memory (`host/src/chat/memory/`, crate `host/crates/memory`)

Memory records the person, not the work; what a bot learns about a project goes in a note of its own. The
[design](../design/bot-memory.md#memory-is-files) explains why. Memory is Markdown files the bot keeps itself, like
Claude Code's own memory; the rules live in the `codync-mem` crate, and `chat/memory` runs them in the host.

- **Two layers, two kinds of file.** Each layer is a folder: "yours", shared by every bot that uses it, in
  `<data>/shared-memory/`, and one per bot in `<data>/bots/<id>/bot-memory/`. A bot in its own workspace uses
  "yours" by default and one in a project folder doesn't; the user switches it per bot (`sharedMemory`).
  - `MEMORY.md`: the short version, one short fact per line (at most 150 tokens a line), the way a project keeps
    its `CLAUDE.md` short and its `docs/` detailed. Caps count tokens (one per CJK character, one per four others):
    400 for "yours", 1,200 per bot. It may be empty. Dreaming keeps it within its cap (below).
  - Notes: the detail, one topic each, `lowercase-with-dashes.md` in up to three topic folders
    (`health/allergies.md`), starting with frontmatter whose `description:` says when to read it; at most 2,000
    tokens a note and 20 notes a layer, marked when passed, never cut. Links (a topic folder too: the host never
    reads or writes through one), hidden files and folders, and folders deeper than three are ignored.
- **The opening** holds each `MEMORY.md` the bot sees in full (over its cap: marked, never cut), then the index of
  its notes (path, description, tokens). It doesn't change for a memory change, so the prompt cache holds: every
  change (the bot's, another bot's in "yours", dreaming's, an edit on the computer) reaches each session of the
  bots that see it on its next message as a `<memory_update>` with the whole memory. The session whose own change
  it was isn't told. A ledger in `kv` keeps, per session, the revision its opening holds and the one it has heard;
  after a compaction the second falls back to the first, so whatever is newer is told again.
- **One way to write, for every harness.** The bot changes memory by editing the files:
  - The built-in `memory` MCP server is the same for every harness: `read_memory` (a file, or the list),
    `edit_memory` (`append` a line, `old`/`new`, the whole `content`, or `delete`), `search_memory` (lines of the
    files; a `MEMORY.md` line names the message it was learned from as `source`) and `search_history`.
  - Agents that take ACP `additionalDirectories` (Claude, Codex) get both folders on every session request and may
    use their own file tools instead. A tool call that changes a file there (an edit, delete or move, or one with
    diffs, naming the path) is noted with the turn it came in; a change no tool call named (a shell command) is
    still the bot's when it turns up during a turn of the bot that ran a tool able to write files (an edit,
    delete, move or command).
  - Memory writes go in at once whatever the bot's permission: memory isn't an action on the user's computer.
    Nothing about memory shows in the chat.
- **What a bot may change** (`codync_mem::files::admit`). What every change brings in passes the checks
  (`codync_mem::files::check_change`): no hidden characters, prompt-carrying links, planted instructions,
  passwords, keys, ID or card numbers, and the caps. Lines already there aren't checked again, and a file over its
  cap may still shrink. In a turn the user started in the bot's own chat (`Runtime.message`; a message the
  host wrote in the user's place doesn't count), the bot may change anything, and `MEMORY.md` may grow to twice
  its cap, so what the user asks to remember is never turned away for space. Otherwise (routines, rooms, other
  bots) it may only add lines, none the user had forgotten, and within the cap: a full `MEMORY.md` sends the bot
  to a note instead.
- **Forgetting.** When a change in the user's own turn takes a fact out of a layer (and no other file of the layer
  holds it), that fact and the message it was learned from are tombstoned (`codync_mem::store::bury`): no bot
  outside the user's turn and no dream writes it again or learns from that message. Written again in the user's
  own turn, it may come back. Earlier versions are kept 30 days (`files::prune`, once a day), so what was forgotten
  is gone from the history after that.
- **The scan** at the start and end of every turn, on the memory screen and before a tool call records each change
  as a version with its full content: mtime and size first, then SHA-256; a file younger than 2 s waits (the
  turn-end scan looks again once it settles). A bot's change that breaks a rule is put back, and the session that
  made it hears why at the start of its next message (`memory::put_back`). A change is the bot's when its tool call named the file,
  or when it turns up from the bot's prompt until the scan after its turn, and that turn ran a tool that can write
  files (`turns::begin_turn`; when several bots' turns could have made it, the strictest rules apply). Any other
  change (an editor, another program) is recorded as the computer's, unchecked; the opening withholds each of its
  lines that fails the checks. Host writes (`edit_memory`, dreaming, the moves below) are compare-and-swap on the
  content's hash, one at a time with the scans, through a temporary file renamed over it. Each fact of `MEMORY.md`
  keeps who wrote it, when, and the message it was learned from (`memory_facts`).
- **Dreaming** (`chat/memory/dream.rs`). After five idle minutes or eight user messages (sooner on a new session or
  compaction, and once a day for the time review, which rewrites a dated line whose date has passed, once), it
  reads the bot's chat since its bookmark on the bot's own harness (Claude: Haiku, no tools) and proposes lines to
  add to and take out of `MEMORY.md` and lines to append to notes. Each change must quote what the user typed:
  pasted text (the apps send `pasted`), fenced code, quoted lines and forwarded messages don't count, nor do bare
  acknowledgements. At most 10 changes and never more than a quarter of the lines taken out; it never takes out a
  line the user wrote, never writes one the user had forgotten (an add, a review's rewrite, a pointer below), and
  never rewrites a note.
  **Tidying:** when a `MEMORY.md` the bot sees is nine-tenths full (`codync_mem::dream::full`), a run moves its
  detail word for word into the notes of their topics (`moveOut`), a topic's lines together, the user's lines too,
  and leaves one line per topic that sums it up and points to its note, the way `CLAUDE.md` points into `docs/`
  (`Allergic to peanuts → health/allergies.md`): the run's `summary`, else the note's description, else its name,
  the first that passes `MEMORY.md`'s checks and fits; when none does, the topic gets no line and the index still
  lists the note, so a pointer never costs the run its other changes. A topic that already points to its note
  keeps that line unless the run sums it up anew and the user didn't write it; a pointing line never moves itself.
  A move keeps the line, so it doesn't count toward the quarter. It runs for this even with no new conversation,
  and again after each run that moved something, until there is room or a run moves nothing (`tidied` in `kv`
  holds, per layer, the hash of the `MEMORY.md` a run last saw nearly full). Runs of bots that see the shared
  layer take turns, so one run tidies it and the others find it tidied. It waits while the plan's usage is above 75%. It writes the files
  compare-and-swap; when one changed meanwhile, the rest waits for the next run. Every run is logged (the lines it
  added and took out, the notes it wrote).
- **The harness's own memory** is off for every bot and for background jobs (`agent/own_memory.rs`): Claude Code
  (`autoMemoryEnabled` and `autoDreamEnabled` false, `CLAUDE_CODE_DISABLE_AUTO_MEMORY`), Copilot (its memory tools
  excluded), CodeBuddy, WorkBuddy (the same switch as CodeBuddy), and Qwen Code (workspace settings, so only in a bot's own workspace: a project folder's
  files are the user's). Gemini CLI, Kilo Code, Kiro and Hermes Agent have no switch Codync can use. Project
  knowledge goes in the bot's own notes, and into the project's `CLAUDE.md` or `AGENTS.md` only when the user asks.
- **Keeping it on.** Where Codync has the switch, the memory screen's "This bot" section offers "<harness>'s own
  memory" (bot field `harnessMemory`, `null` without a switch), off by default, with an info button: the bot may
  remember things twice, the two memories can disagree, and forgetting something in Codync doesn't reach it.
  Changing it restarts the agent, which resumes its session where it can, and moves `memoryRev`. Kept on, the
  harness runs its memory as it sets it, and Claude Code's `MEMORY.md` shows read-only in a "Claude Code" section
  (empty until it saves something). An edit to another harness turns the switch off. Using the harness's memory
  instead of Codync's is planned: [harness memory](../plans/harness-memory.md).
- **Shared by every bot.** Gemini CLI (`~/.gemini/GEMINI.md`) and Hermes Agent (`~/.hermes/memories`) keep one
  memory for all their sessions that Codync can't turn off (`own_memory::shared`, bot field `harnessMemoryShared`).
  In place of the switch, "This bot" shows "<harness>'s own memory · Shared" with an info button (the host doesn't
  read whether the user turned it off in the harness's settings, so the row names no state): every bot on that
  harness reads and writes the same notes, and so does the harness outside Codync; forgetting in Codync doesn't reach
  them, Codync can't show them, and a harness setting, where there is one (Hermes Agent's `memory.*` flags; Gemini CLI has none), turns them off for every session at once.
  The terminal UI says it in the section's note. Kilo Code and Kiro aren't marked: how far their memory is shared
  isn't confirmed.
- **The apps** (iPhone, desktop, terminal) show "About you", "This bot", the dream log and Claude Code's notes,
  read-only: the user changes memory by telling the bot in the chat. Each layer shows its `MEMORY.md` facts with
  who wrote them (the `memory` method keeps the shape of the earlier entries: one per fact, `state` active), then
  its notes (the terminal: a Notes section) with description, size and date, marked past a cap; a note opens to
  read. The dream log shows added lines green and taken-out ones red, beside an info button (the terminal: a short
  paragraph above it) saying what dreaming does. The switches stay: "Use what your bots share" and the harness's
  own memory. They fetch with `memory` and `memoryFile`, and fetch again when the bot's `memoryRev` moves. The
  apps' edit methods of 2.18 to 2.20 are gone, so the host's `minApp` is 2.21.0
  ([floor history](../reference/compatibility.md#floor-history)).
- **Moving the old memory**, once, on the first start of a host that has it:
  - the memory entries of 2.18 to 2.20 (`codync_mem::legacy`): each layer's active entries go into its `MEMORY.md`
    as far as the cap allows (pinned first, then the newest), each with who wrote it and its source message; the
    rest, the archived ones and any too long for one line go into a note, `earlier.md`. Entries a newer one
    replaced, and facts the checks refuse (secrets, planted instructions, hidden characters), stay out. Old dream
    runs keep their lines. Then the entries' tables go.
  - each bot's Engram database (or the Markdown memory before it), the same way. Then the old files (deleted bots'
    too) and the Engram download go, and dreaming starts from that point in the chat.

## Compaction and rollover (`host/src/agent/bot/compaction.rs`, rules in `codync_mem::compaction`)

- **What the host records**, per session in `kv` (`compaction.<bot>.<session>`): context use and window size
  from `usage_update`, compactions, and when turns end. How long the provider keeps prompts cached is learned
  from turns that came after a pause (`cache.life.<bot>`); until then it is an hour for Claude on a subscription
  and five minutes otherwise. Each turn's cached read and write tokens are logged (`turn usage`).
- **Compacting ahead of time**, in the bot's own chat only (never rooms, routines or bot requests):
  - at the end of a turn past three quarters of the window or 200k tokens, whichever is less;
  - while idle, with a one-hour cache and over 100k in use, five minutes before the cache expires.

  It runs as a hidden turn: dreaming first, then the opening rendered again (a Claude session reloads only when
  it changed), then `/compact <keep-list>` for Claude, or `/compact` for Codex, whose keep-list is its
  `CODEX_CONFIG` `compact_prompt`. Other harnesses compact on their own. The keep-list is also in the opening.
- **Loop guard:** after three host compactions in a day, or one that left 60% of the trigger in use, or one
  fewer than five turns ago, the host rolls over instead of compacting again.
- **Rollover:** also due when a conversation over 100k went cold (its cache expired) or after eight hours away
  with three compactions behind it. A handoff card appears in the chat (`chat/handoff.rs`): the last summary,
  the user's last four messages and the last reply (each cut at 2,000 characters, so a pasted document doesn't fill
  the new conversation), the open plan, and a bookmark for `search_history`. The user can
  edit it (`editHandoff`) or keep the conversation (`keepConversation`); otherwise their next message there starts
  a new session that begins with it.
- **After every compaction** the host records a notice ("Earlier context was summarized to make room.", its
  `compaction` field carrying Claude's summary, which the next handoff starts from). The apps don't show it: the
  same conversation goes on.
- **In the apps** the chat explains only a fresh conversation, in one line, left-aligned like the bot's messages.
  The iPhone and desktop show the draft card with *Keep this conversation* and a pencil that opens the editor
  sheet; once sent it folds to "Started a fresh conversation with this summary.", which unfolds to what it
  carried, and once kept it leaves no line. In the terminal, pick the line (`v`, `j`/`k`), then `o` unfolds it,
  `e` edits and `K` keeps the chat. The user starts a new session from the bot's menu (terminal: `S`), not with a
  `/` command.

## Turns

- Messages sent while the agent works are folded into one next turn, joined by blank lines.
- Bot-to-bot requests and the replies to them occupy separate turns in the same queue (replies waiting together are one turn); user messages are only folded together up to the next request or reply. Neither feeds dreaming. See [bot collaboration](bot-collaboration.md).
- A request turn (the recipient's) doesn't set the restart marker: its pending notices become interrupted errors instead of automatically replaying work. A reply turn is the sender's own chat turn and sets it, so it resumes like a user turn.
- Stop sends `session/cancel`; an agent still running 15 seconds later is stopped (it resumes its session on
  the next turn). `session/new`, `session/load` and `session/fork` get three minutes before the agent is
  stopped. Connector or computer changes restart an agent that can resume sessions before its next turn; one
  that can't keeps its session, and its next new session gets them.
- `turn.inflight.<bot>` records when the running turn in the bot's chat started. If the host stops mid-turn (update, crash, restart), the next
  start resumes that session with a hidden "you were interrupted, don't repeat finished steps" prompt. If the
  session can't be resumed, or the turn started over an hour ago, it does nothing.

## Limits

- An agent without ACP `systemPrompt` gets the instructions as a user message, which models follow less firmly
  than system text.
- The harness writes every summary. Codync chooses when the bot's own chat compacts and what it asks the summary
  to keep, and re-applies the instructions afterwards; it can't make a harness keep anything.
- A session a harness has deleted (for example, one cleaned up after a month) can't be resumed. The bot
  starts fresh, but its memory remains.
- Memory search is a substring scan, not semantic vectors. A voice call's transcript feeds dreaming from the call's
  line: what the user said counts as their own words (spoken, not typed), the voice's lines and what it relayed to
  the bot (`from: voice`) are context only ([voice call](voice-call.md#the-call-on-the-host)). Group rooms don't
  feed dreaming.
- `search_history` covers the bot's own chat, not group chats it took part in. It finds messages by words, or
  reads back from a message id (`from`, then each page's `next`); `search_memory` gives each `MEMORY.md` line's
  source message, so a bot can check where a memory came from.
- A bot told by something it read in the user's own turn (a web page, a file) can still change or take out
  memory; routines, rooms and other bots can't.

A reply stays in its chat's session, and its quote stays out of what memory takes as the user's words; see [groups and replies](groups-and-replies.md). Group and delegated requests don't feed dreaming.
