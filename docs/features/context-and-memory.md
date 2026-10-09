# Context and memory

How a bot keeps its instructions, context and long-term memory. Engram owns durable memory; Codync manages
capture, recall and the keeper. The ACP harness owns the conversation and compaction.

## Transcript, context and memory

| | Where | Owner | Lifetime |
|---|---|---|---|
| Transcript | SQLite `entries` | Codync | Persistent main chat and flat reply threads per bot |
| Model context | ACP session (`bots.session_id`) | The harness | Until *New session*, a failed resume, or an agent/command/folder change |
| Long-term memory | `~/.codync/bots/<id>/engram/engram.db` | Engram + Codync keeper + the user | Forever, across sessions |

Codync never replays the transcript into a session. Each turn sends only the new message(s); the harness keeps
its own context and compacts it itself.

## Instruction snapshot (`host/src/chat/context.rs`)

- The bot's instructions are rendered once, in English and kept general: profile (name, description), how to
  message the user, other conversations, workspace, skills, plus the memory section. How to use each tool
  lives with the tool (MCP server instructions, tool descriptions), not here. They're stored as a snapshot in `kv` under `context.<bot>`, keyed by **session + compaction
  epoch**, and don't change until one of those changes. This keeps the prompt byte-identical so the prompt
  cache stays warm.
- **Claude** (`agentCapabilities._meta.claudeCode` present) receives the snapshot as a real system prompt:
  `_meta.systemPrompt.append` on `session/new` and `session/load`. It survives compaction.
- **Other harnesses** receive it in the first message of each session (`<bot-profile>…</bot-profile>`).
- **Compaction epoch:** Codync advertises `clientCapabilities.session.compaction`. Each completed
  `compaction_update` (deduplicated by `compactionId`) increments `context.epoch.<bot>`. The next turn
  re-renders the snapshot, and Claude's session is loaded again with the new system prompt. The adapter
  rebuilds its query and resumes the same conversation.
- **Profile edits mid-session** (name, description, skills) are sent once as an `<agent_profile_update>` block
  appended to the next message. After that turn reaches the agent, the change is recorded in
  `snapshot.announced`. The next compaction folds it into the snapshot.

## Memory (`host/src/chat/memory/`)

- Engram stores each bot's durable observations in an independent SQLite database. The old Markdown memory
  imports automatically, with original dates and source references preserved.
- The instruction snapshot includes up to 100 profile facts and 30 recent facts, within a 4,000-character
  recent-memory budget. A keeper consolidates crowded profiles to at most 60 facts; demoted facts remain
  searchable. Manual corrections invalidate snapshots and send a notice on the next turn.
- Completed memorable exchanges queue durably. After five idle minutes or eight exchanges, the bot's own
  harness extracts durable facts and saves a native Engram session summary. A new session or reported
  compaction requests an early flush. Failed batches remain queued for retry.
- Claude's keeper uses Haiku without tools, settings or persisted helper sessions; other harnesses use the
  bot's selected model with inline instructions. Automatic naming still uses `chat/naming.rs`.
- The built-in memory MCP server forwards native Engram tools for recall, saving, review and conflict
  judgment, and retains `search_history` for the bot's original main-chat and thread messages.
- iOS, desktop and terminal provide search, CRUD, pinning, review, history, timeline, export and import.
  See [Engram memory integration](engram-memory.md) for storage, migration, deletion semantics, API and tests.

## Turns

- Messages sent while the agent works are folded into one next turn, joined by blank lines.
- Bot-to-bot requests occupy separate turns in the same queue; user messages are only folded together up to the next request. Requests never feed the user-fact memory keeper. See [bot collaboration](bot-collaboration.md).
- Delegated turns do not set the restart marker: their waiter disappears on restart. Pending delegation notices become interrupted errors instead of automatically replaying work.
- `turn.inflight.<bot>` records the running turn start and its thread lane. If the host stops mid-turn (update, crash, restart), the next
  start resumes that session with a hidden "you were interrupted, don't repeat finished steps" prompt. If the
  session can't be resumed, or the turn started over an hour ago, it does nothing.

## Limits

- Harnesses other than Claude have no system prompt channel and don't report compaction. For them, the first
  message carries the instructions and the snapshot only refreshes when a new session starts.
- Compaction is the harness's own. Codync can't choose what survives it, only re-apply its instructions
  afterwards.
- A session a harness has deleted (for example, one cleaned up after a month) can't be resumed. The bot
  starts fresh, but its memory remains.
- Memory scopes stay inside a bot's database; user-level sharing across bots is not implemented. Search uses
  FTS5 rather than semantic vectors. Consolidation runs when the profile outgrows the prompt, not daily.
- `search_history` covers the bot's own chat, not group chats it took part in.

Reply threads have separate session/context lanes; see [groups and threads](groups-and-threads.md). Group/delegated requests do not become user facts in the memory keeper.
