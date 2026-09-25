# Context and memory

How a bot keeps its instructions, context and long-term memory. The mechanisms follow Grok Bot's design
(`system-prompt-assembly`, `sand-memory`, `turn-memory`, `upgrade-recreate-resume`) and are adapted to ACP,
where the harness owns the conversation.

## Two memories

| | Where | Owner | Lifetime |
|---|---|---|---|
| Transcript | SQLite `entries` | Codync | Forever, one endless thread per bot |
| Model context | ACP session (`bots.session_id`) | The harness | Until *New session*, a failed resume, or an agent/command/folder change |
| Long-term memory | `~/.codync/bots/<id>/memory/` | Codync (keeper) + the user | Forever, across sessions |

Codync never replays the transcript into a session. Each turn sends only the new message(s); the harness keeps
its own context and compacts it itself.

## Instruction snapshot (`host/src/context.rs`)

- The bot's instructions are rendered once: profile (name, description, skills, phone-friendly rule) plus the
  memory section. They're stored as a snapshot in `kv` under `context.<bot>`, keyed by **session + compaction
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

## Memory (`host/src/memory.rs`)

- Facts are plain markdown, one `- (YYYY-MM-DD) fact` per line:
  - `profile.md` holds who the user is. All of it goes into the prompt, up to 100 facts.
  - `log/YYYY-MM.md` holds dated history, including `[note]` and `[episode]` lines. The newest 30 go into the
    prompt, within 4,000 characters.
- **Keeper:** after a turn ends normally, if the user's message is memorable (not "thanks"/"ok"), a one-shot
  agent of the bot's own harness extracts facts using Grok Bot's extraction prompt
  (`profile:` / `log:` / `note:` / `remove:`).
  - On Claude it runs with a replaced system prompt, no tools, no settings, `persistSession: false` and the
    `haiku` model.
  - Other harnesses get the instructions inline, in `~/.codync/memory-keeper`.
- **Episodes:** every 6 remembered exchanges (pending turns in `memory.episode.<bot>`), the keeper writes one
  `[episode]` journal sentence.
- The agent is told where its memory folder is so it can grep older facts. Facts learned mid-session reach its
  prompt at the next compaction or session.
- **API:** `memory`, `forgetMemory`, `clearMemory`. The Memory card in bot settings lists and removes facts.

## Turns

- Messages sent while the agent works are folded into one next turn, joined by blank lines.
- Bot-to-bot requests occupy separate turns in the same queue; user messages are only folded together up to the next request. Requests never feed the user-fact memory keeper. See [bot collaboration](bot-collaboration.md).
- Delegated turns do not set the restart marker: their waiter disappears on restart. Pending delegation notices become interrupted errors instead of automatically replaying work.
- `turn.inflight.<bot>` holds the start time of a running turn. If the host stops mid-turn (update, crash, restart), the next
  start resumes that session with a hidden "you were interrupted, don't repeat finished steps" prompt. If the
  session can't be resumed, or the turn started over an hour ago, it does nothing.

## Limits

- Harnesses other than Claude have no system prompt channel and don't report compaction. For them, the first
  message carries the instructions and the snapshot only refreshes when a new session starts.
- Compaction is the harness's own. Codync can't choose what survives it, only re-apply its instructions
  afterwards.
- A session a harness has deleted (for example, one cleaned up after a month) can't be resumed. The bot
  starts fresh, but its memory remains.
- User-level memory shared across bots, project memory and the daily memory "dreaming" pass from Grok Bot
  are not implemented.
