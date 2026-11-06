# Bot memory and compaction

Status (2026-10-21): the whole [rollout](#rollout) is built on 2.18.0. That is the cards, the move from
Engram, writes by permission, the checks, dreaming and all three apps (M0); instructions for every harness (M1);
the cache-safe opening with host-run compaction and the handoff (M2); long-form memory files (M3); and the
evaluation, run once (M4). The public mirror (M5) is live at `leepokai/codync-mem`. Since 2.21.0 the apps are
read-only and [memory is files](#memory-is-files): the cards became each layer's `MEMORY.md`, and the bot edits
memory like any file, through one set of tools in every harness. How it works today is in
[context and memory](../features/context-and-memory.md); the research behind it is in
[bot memory research](bot-memory-research.md). The numbers are starting values, tuned against the
[evaluation](#evaluation).

Left before release, found 2026-10-18 and put off by the owner for now:

- **The Android app's memory screen errors.** It still calls the removed Engram methods and has none of the new
  memory UI. Android isn't released, so it doesn't hold the release; the port is listed in
  [the Android plan](../plans/android-app.md#not-ported-yet).
- **Full e2e on Linux and Windows** hasn't run; only macOS has.
- **Bots inherit the owner's own Claude Code setup** (`~/.claude/CLAUDE.md`, permission mode, claude.ai
  connectors); isolating them waits for the owner's decision ([evaluation](#evaluation)).
- **The move from old memory**, tried on a copy of the owner's data (149, 188 and 3 entries moved, none lost):
  - Old Markdown memory is mostly the bot's own replies ("需要我幫什麼忙嗎？"), and it comes in active up to
    the cap. Only profile, pinned and personal-scope facts should come in active; the rest archived.
  - The old files are deleted once moved, with no backup; entries refused by the checks vanish with them.
  - An empty (0-byte) `engram.db` fails the move on every start, so that bot never finishes moving.

Built differently from the plan below, for now:

- Search is a substring scan, not an FTS5 trigram index. Memory has a few hundred entries; the transcript scan
  took 0.6 s over 180k messages of one bot, so the index waits until a bot's search passes a second.
- No proposals: since 2026-10-18 every bot writes memory at once, and nothing about memory shows in the chat.
- Health, religion and politics get no special notice yet; voice transcripts aren't marked, and group rooms
  don't feed dreaming.
- Facts show who wrote them, not a link to the message they were learned from. The bot can follow it:
  `search_memory` returns a `MEMORY.md` line's source message, and `search_history` reads back from a message id.
- A memory notice carries the whole memory (1,600 tokens within the caps, up to twice that after the user's own
  turns until dreaming tidies it) rather than a delta: a whole copy also replaces whatever an earlier notice said.
- The cache's lifetime is learned from turns that come after a pause (most of the input read from cache, or
  not); until a bot has one, Claude on a subscription counts as an hour and everything else as five minutes.
- The host compacts only Claude and Codex ahead of time; other harnesses keep their own timing. Codex's
  compaction line in the chat has no summary, since Codex reports none; its handoff is built from the chat.
- The handoff card appears as soon as a rollover is due, not when the user returns, so it can be edited before
  the message that sends it.
- A memory file changed outside every bot's turn (an editor, another program) is recorded without the checks or a
  chat line: the user owns their computer. A line of it, or its description, that fails the checks is left out of
  the opening.

In one line: **memory records the person, the transcript stays in the host, the opening stays frozen until
compaction, and the host decides when to compact.**

## Why redesign

Reading the current implementation turned up these problems.

- **Two writers for one memory.**
  - The agent writes through Engram's MCP tools, and the keeper extracts facts from the same exchanges.
  - Each keeper batch runs the agent twice.
  - User messages are stored three times: transcript, keeper queue, and Engram's captured prompts.
  - Each turn makes three Engram round trips.
- **The mid-session reload doesn't work.**
  - Claude Code records the system prompt on a session's first request and reuses that record until the session
    is compacted. This is on by default since Claude Code 2.1.265 (SDK 0.3.265) and no longer flag-gated since
    2.1.268. Local transcripts already contain `systemPromptSnapshot` records.
  - When `_meta.systemPrompt` changes, `claude-agent-acp` tears down the query and resumes it, which replays history
    and re-runs SessionStart hooks. The new text still takes effect only at the next compaction.
  - So "memory changed, `session/load` with a new append" restarts the process for nothing.
- **The opening is one compaction late.** The host reacts to `compaction_update` completed by re-rendering and
  loading the new opening. The compaction that just ran already re-recorded the old prompt, so the new one is
  picked up only at the following compaction.
- **Non-Claude harnesses lose the profile.**
  - `<bot-profile>` is sent only in a session's first message (`host/src/agent/bot/turn.rs`).
  - Codex keeps only the last 20k tokens of user messages verbatim when it compacts locally
    (`COMPACT_USER_MESSAGE_MAX_TOKENS`). Remote compaction keeps developer messages only behind a feature flag.
  - `codex-acp` passes developer instructions only on `session/new`, not on load or resume.
  - Codync re-sends the profile in none of these cases.
- **Fork drops the opening.** `session/fork` sends no `_meta`, and the adapter forwards none.
- **Adapter signals are ignored.** Both `claude-agent-acp` and `codex-acp` report:
  - `usage_update{used,size}`;
  - per-prompt `cachedReadTokens` / `cachedWriteTokens`;
  - compaction progress.
  
  Claude also sends `compact_boundary` token counts and the summary. The host reads only
  `_meta["_claude/rateLimit"]` (`host/src/agent/bot/updates.rs`).
- **A second memory runs unseen.** Claude Code's auto memory and auto-dream run inside bot sessions. Auto memory is
  "loaded into the system prompt at session start", and none of it is visible in Codync.
- **Engram has the wrong shape.** The details are under [Engram](#engram).
  - Engram is built for one coding agent remembering its own work: one database per bot, written only by Engram,
    with Codync reading its schema read-only.
  - That rules out the shared "yours" layer, the write gate, provenance with cascade delete, a dream log with undo,
    and memory writes stamped with `rev` in the same transaction.

## Patterns explored

Two research passes on 2026-10-17 compared the following; a third pass on how open-source agents mix files and a
database, embeddings, and harness instructions is in [bot memory research](bot-memory-research.md).

- **Products:** ChatGPT, Claude, Grok (Grok Bot, Grok Build), Gemini, Codex, Claude Code, Cursor, Amp, Factory,
  OpenClaw, Hermes, Letta, Mem0, Zep/Graphiti.
- **Papers** from 2024–2026.
- **Community threads:** Hacker News, Reddit and GitHub issues.
- **Local sources:**
  - `claude-agent-acp` 0.92.0;
  - Claude Agent SDK 0.3.300, which bundles Claude Code 2.1.300;
  - `codex-acp` on GitHub.

### Where memory lives and who writes it

| Pattern | Seen in | What we found | For Codync |
|---|---|---|---|
| Agent-managed memory through tools | Letta/MemGPT core memory, Engram MCP, Hermes | Agents forget to call the tool, or say "remembered" without calling it. Tool writes bypass any review. Letta shared blocks are last-write-wins. | Not the main writer. The bot's writes follow its permission mode, and every write is visible with undo. |
| Background extractor | Mem0, ChatGPT saved memories, our keeper | One background writer is the consensus. Mem0 went add-only in April 2026 because reconciliation "was where context got destroyed". | Kept as **dreaming**, limited to add and archive. |
| Small profile always in context | ChatGPT saved memories, Claude memory summary, Hermes `MEMORY.md`/`USER.md` | Reliable, because nothing depends on the model deciding to look. Needs a size cap. Hermes' 2,200-character cap caused silent fact loss. | **Adopted** for active memory, capped in tokens. |
| Retrieval on demand | Vector RAG, our former "context" section | A community evaluation found about 60% of failures were the agent never searching. | Only for archived memory and the transcript. |
| Raw transcript search | Letta's filesystem benchmark (grep agent ≈74%) | Beats most memory products. Replacing the transcript with summaries or extracted facts lowers accuracy (LongMemEval). | **Adopted**: `search_history` over the host transcript. |
| Knowledge graph / temporal graph | Zep/Graphiti, A-MEM | Graphiti invalidates old facts instead of deleting them. Benchmark gains are small next to benchmark noise: 6.4% of LoCoMo's answer key is wrong. | Not built. We borrow "supersede, don't delete". |
| File-based memory | CLAUDE.md, Claude Code auto memory, Codex memories, OpenClaw `MEMORY.md` | Transparent and editable, but no transactions and no sync to three clients. | SQLite, made fully visible in the UI. Harness file memory is off unless the user keeps it on for a bot. |
| Sleep-time consolidation ("dreaming") | ChatGPT dreaming, Claude Code auto-dream, OpenClaw, Letta sleep-time, Grok Build `/dream` | Useful, and every product adds guards. OpenClaw rejects a rewrite that removes more than 0.25 of entries and keeps a pre-image. Hermes removed its `replace` action because it wiped other facts, and #5563 traced lost memory to corruption and the cap. Claude Code auto-dream keeps "no log of what was created, modified, or removed" (#38493). | Adopted, with the guards below. |
| External memory service | Engram, Mem0 cloud, Supermemory, Honcho | A separate process owns the writes and has its own failure modes. | Dropped. |

### Scoping across bots

| Product | Scope |
|---|---|
| Grok Bot | Per bot: "Memory and Routines belong to the Bot … context remains with the role" |
| Claude Projects | Per project, with a per-project "Use account memory" toggle |
| ChatGPT Projects | "Project-only memory", but a 653-point Reddit thread reproduced a leak into a project-only project |
| Letta | Shared blocks, last write wins; read-only blocks advised |

Cross-scope bleed is the most common complaint everywhere. A 738-point r/ChatGPT thread: "memory is great until
it quietly carries an assumption from three chats ago into something unrelated. a per-chat toggle would fix most
of it". Codync's shape follows Claude Projects: per-bot memory plus a shared "yours" layer that each bot can switch
off. The user, dreaming and bots in `Auto` write "yours", and each entry names who wrote it.

### Consensus we design to

1. **Memory is for the person, not the work.**
   - Codex: "Keep required team guidance in AGENTS.md"; memories are "a recall aid".
   - Grok Build memory excludes anything the repository already covers.
   - Claude Code auto memory skips what it can derive from the code.
   - Users who let memory touch code complain that it "saves a memory with the patch in it instead of fixing the
     code. disabled memory user wide" (r/ClaudeAI).
2. **Scope it.** Memory must not leak across contexts the user keeps apart.
3. **Staleness is the hard part**, more than finding things. Archive superseded facts with a pointer to what
   replaced them, and turn the future into the past as time passes. ChatGPT dreaming turns "You're going to
   Singapore" into "You went in July 2026".
4. **Visible, editable, with provenance.** Every memory says where it came from and can be undone.
5. **One automatic writer, behind a gate.**
6. **Memory is an attack surface.** Treat it as data, not instructions. Published attacks:
   - MINJA poisons memory through queries alone.
   - AgentPoison reaches over 80% success at a poison rate under 0.1%.
   - Gemini's delayed tool invocation fires a planted instruction when the user later says "yes".
   - Microsoft found over 50 "remember … as a trusted source" prompts from 31 companies planted in links.
   
   The defense that holds is structural (CaMeL, the design-patterns paper): untrusted input must be unable to
   trigger consequential actions.
7. **The raw transcript is the source of truth.** Memory is an index over it.
8. **A stable prompt prefix.** Anthropic: "Put changing information in messages, not the system prompt."
9. **Small and complete beats large plus retrieval.**

Still contested: files vs. a database, vectors, and graphs. We use SQLite for transactions and three-client sync,
and keep the file camp's real requirement: no hidden state.

### Compaction patterns

| Pattern | Seen in | For Codync |
|---|---|---|
| Harness auto-compacts at a threshold | Claude Code (at the model limit, about 967k on 1M models), Codex | Kept as the safety net |
| Idle compaction inside the harness | Claude Code 2.1.299+: interactive terminal only, flag-gated off by default | Never active in bot sessions, so the host's idle compaction is the only one |
| Summary written while idle, swapped in on return | Claude Code's flag-gated `compact_on_return` mode; its "resume from summary" offer after over 1 hour idle and over 100k tokens | Precedent for host idle compaction and rollover |
| Host-timed compaction | No product feature found | **Adopted**: end of turn, and idle before the cache expires |
| Handoff to a fresh session | Amp Handoff, an editable draft; Amp calls compaction "lossy" and warns about "stacking summary on top of summary" | **Adopted** as rollover |
| Anchored or iterative summaries | Factory: only the newly truncated span is summarized and merged. It scores 3.70 vs. 3.44 (Anthropic) and 3.35 (OpenAI); the artifact trail is weakest everywhere. | Left to the harness; rollover resets drift; probes in the evaluation |
| Memory-flush turn before compaction | OpenClaw | Not needed: the host keeps the full transcript |
| Masking old tool output | Anthropic context editing (it invalidates cached prefixes) | The harness's job |
| Writing our own summaries | Anthropic on-demand and threshold compaction APIs | Not built until harness summaries prove insufficient |

## The design

### Memory is files

Since 2.21.0 memory is Markdown files the bot keeps itself, the way Claude Code keeps its own `MEMORY.md`. Until
then the always-loaded part was cards in SQLite, written through fixed tools (`remember`, `revise_memory`,
`archive_memory`) and edited by the user in the apps. The apps became read-only (the owner's call: the user
changes memory by telling the bot), and with that the cards' reasons went: no per-card pin, archive or undo in a
UI, and no "the user's own words" to keep out of a bot's reach. What was left was two write paths for one memory,
so they became one.

**Two layers, two kinds of file.** Caps count tokens, not characters, so Chinese users aren't penalised: one token
per CJK character, one per four Latin characters.

| | `MEMORY.md` | Notes |
|---|---|---|
| Holds | One short fact per line: name, time zone, people, "don't …", commitments | One topic's detail: people, health, plans, trips |
| Cap | 400 tokens for "yours", 1,200 per bot; 150 a line | 2,000 tokens a note, 20 notes a layer |
| In the opening | In full | An index: path, description, size |
| Names | `MEMORY.md` | `lowercase-with-dashes.md` in up to three topic folders (`health/allergies.md`), frontmatter `description:` first |

| Layer | Folder | Default | Scope |
|---|---|---|---|
| **Yours** | `<data>/shared-memory/` | On for personal bots, off for project bots | Shared by all bots |
| **This bot** | `<data>/bots/<id>/bot-memory/` | Always on | One bot; wins over "yours" when they conflict |

Active total: 1,600 tokens per opening within the caps, up to twice that after the user's own turns until dreaming
tidies it (Hermes about 800 + 500, Claude Code's `MEMORY.md` index 200 lines, Letta blocks 5,000 characters). A file past a cap is marked in the opening and the apps, never cut.
Project knowledge stays in the harness's files (`AGENTS.md`, `CLAUDE.md`) or a note on the project.

**Memory is never full.** `MEMORY.md` relates to the notes the way a project's `CLAUDE.md` relates to its `docs/`:
the short version in every session, with one line per topic that sums it up and points to its note
(`Allergic to peanuts → health/allergies.md`), and the detail by topic in folders. Dreaming keeps it that way: once
a `MEMORY.md` is nine-tenths full it moves detail out into topic notes and leaves the pointing line (see Dreaming). What the user asks a bot to remember in
their own chat is never turned away for space: there `MEMORY.md` may grow to twice its cap until dreaming tidies
it. Other turns stay within the cap and write what doesn't fit in a note.

**The file is the live copy.** The bot reads it from disk (its own tools or `read_memory`); the opening renders
from the last recorded version, which a scan keeps current. SQLite keeps the history beside it:

- every version's full content, author and cause (`memory_file_versions`), kept 30 days;
- who wrote each fact of `MEMORY.md`, when, and the message it was learned from (`memory_facts`, keyed by the
  fact's fingerprint), so the apps show provenance and `search_memory` can point back at the message;
- tombstones of what the user had a bot forget (`memory_tombstones`).

Provenance is the host's, not text in the file: a bot tricked by a web page can write a line, but not mark it as
the user's or cite a message that never said it.

**One way to write, for every harness.** The `memory` MCP server is the same everywhere: `read_memory`,
`edit_memory` (`append`, `old`/`new`, `content`, `delete`), `search_memory` and `search_history`. Agents that take
ACP `additionalDirectories` (Claude, Codex) also get the folders and may use their own Read and Edit. Both paths
meet in the same rules:

- `edit_memory` checks before writing and answers with the refusal, so the bot can correct itself.
- A bot's own file tool writes first. The scan at the end of the turn (or at the next scan) checks the change and
  puts it back when it breaks a rule; the session that made it hears why at the start of its next message.
- A change that turns up during a bot's turn (until the scan after it) is the bot's even when no tool call named the
  file, once that turn ran a tool that can write files (an edit, delete, move or command), so a shell command meets
  the same rules. Any other change (an editor on the computer) is recorded as the computer's, unchecked: the user
  owns their computer. The opening shows such a line only once it passes the checks.

Host writes (`edit_memory`, dreaming, the one-time moves) are compare-and-swap on the content's hash, through a
temporary file renamed over the file. A file that changed meanwhile isn't overwritten; the other change is
recorded and the write fails. The host never reformats a file.

### What a bot may change

These checks are deterministic code in the crate (`check`, `files::check_change`, `files::admit`), not another
model's judgment.

- **What memory may say**, on what every change brings in: no hidden characters, prompt-carrying links, planted
  instructions (`ignore previous`, `system prompt`, `you are now`, 忽略之前, 系統提示, …), passwords, keys, ID,
  card or account numbers; the caps (`MEMORY.md`'s doubled in the user's own turn); a note's description. Lines
  the file already had aren't checked again, and a
  file over its cap may still shrink, so a line the user wrote that the checks withhold never blocks the bot's
  changes around it. The list catches known attack wording only; a planted fact in plain words passes, bounded by
  the turn rule below, the provenance the bot can't set, and dreaming's review.
- **Who asked.** In a turn the user started in the bot's own chat, the bot may add, change and take out lines.
  In any other turn (routines, webhooks, rooms, other bots, a message the host wrote in the user's place) it may
  only add lines. That keeps a routine or another bot that read a planted instruction from wiping memory; a bot
  told by a page the user had it read in their own turn still can.
- **Forgetting.** When a change in the user's own turn takes a fact out of a layer, and no other file of the layer
  holds it (it may have moved into a note), the fact and the message it was learned from are tombstoned. No bot
  outside the user's turn adds it again, and dreaming neither adds it nor learns from that message. Written again
  in the user's own turn, the tombstone goes. Versions older than 30 days are pruned, so a forgotten fact is gone
  from the history after that.

Source policy for dreaming:

| Source | Can create memory |
|---|---|
| User text in the bot's own chat | Yes, through dreaming's audit |
| User text in a group room | Not yet: rooms don't feed dreaming |
| Voice transcript | Like typed text until transcripts are marked |
| Bot-to-bot requests, bot output, routines, webhooks, files, web pages | Never |

### Dreaming

**Schedule.**

- It runs after five idle minutes or eight queued user messages.
- It also runs before any host-initiated compaction or rollover, and when a `MEMORY.md` the bot sees is
  nine-tenths full, with or without new conversation, until there is room or a run moves nothing. Runs of bots
  that share a layer take turns, so one run tidies it.
- It is skipped while subscription usage is above 75% of the window. Codex keeps the same 25% headroom with
  `min_rate_limit_remaining_percent`.

**How it runs.**

- It uses a fresh, tool-free context on the bot's cheapest model: `tools: []`, Haiku for Claude.
- It reads the transcript from a bookmark forward. Untrusted and system-provenance spans are stripped before the
  proposal prompt (OpenClaw does the same).
- It reads each `MEMORY.md` with who wrote each line, and the notes' index, and doesn't add what is already there.

**Pipeline.** Proposal (`prompt`, `parse`), audit (`audit`), the files' new content (`edits`, which also drops
what the user had forgotten, what memory already has and what doesn't fit), then compare-and-swap writes and the
log. A file that changed while the model worked fails its write; what was written stays, and the rest waits for
the next run.

**Rules:**

- **The user's lines are fixed.** A line the user wrote (moved from the old memory, or edited on the computer) is
  never taken out or rewritten by dreaming; tidying may only move it word for word into a note.
- **Adds, takes out, appends.** Each add and removal quotes the user's own words from one message. Notes only get
  lines appended; dreaming never rewrites a note.
- **Limits.** At most 10 changes per run. A run that would take out more than a quarter of the lines is dropped
  whole (OpenClaw's default is also 0.25); a move keeps its line, in a note, so it doesn't count.
- **Tidying.** When a `MEMORY.md` is nine-tenths full, dreaming moves its detail word for word into the notes of
  their topics (`health/allergies.md`), a topic's lines together, and leaves one line per topic saying what every
  reply needs and pointing to the note (`Allergic to peanuts → health/allergies.md`). A later tidy of the same
  topic rewrites that line rather than adding another, unless the user wrote it.
- **Time review, daily.** A line about a future event whose date has passed is rewritten in the past tense, once;
  its rewrite isn't reviewed again. Relative dates become absolute when written.
- **Dream log.** Every run's added and taken-out lines and the notes it wrote, shown read-only in the apps.

### Frozen opening

The opening, in order:

1. harness tools and preset;
2. identity and rules;
3. yours;
4. this bot.

It stays byte-identical until compaction or a new session.

- **Claude transport.** Send the object form, which the adapter spreads into the SDK preset:
  `_meta: {"systemPrompt": {"append": …, "excludeDynamicSections": true}}`.
  - Not the ACP spec `systemPrompt` field yet: for Claude the adapter rebuilds it as preset plus append and drops
    the other options.
  - `excludeDynamicSections` moves the working directory, git status and similar per-session context into the
    first user message. The opening is then identical across a bot's chat, group turns and routines, and a new
    session reads the cache.
- **Change the opening only at compaction or rollover.** Every change restarts the adapter's query and replays
  history. Resume has had regressions, such as 2.1.288 dropping files a compaction had just restored.
  - The SDK has undocumented mid-session invalidation hooks (`liveAppendSections`,
    `CLAUDE_CODE_HOST_PROMPT_SUPERSEDES_RECORD`).
  - They are unstable and any re-render invalidates the system-prompt cache layer, so the design doesn't depend on
    them.
- **Deltas.** Changes made elsewhere reach the bot as one `<memory_update>` block prefixed to its next message.
  That covers edits on the computer, dreaming, and what other sessions or bots wrote; a change of the bot's that
  was put back rides along with why.
  - Nothing is sent for what this session learned itself, because the bot already heard it.
- **Ledger.** It records two numbers per session: the memory revision the opening contains, and the revision deltas
  have delivered up to.
- **The handoff summary goes in the first user message**, not the opening, because it differs per session.
- **Settings isolation.** Consider `settingSources: []` or `["project"]`. The adapter default
  `["user","project","local"]` loads the user's own `~/.claude/CLAUDE.md` and settings into every bot.

### Instructions in every harness

Codync runs 28 harnesses. The opening, with the bot's identity and memory, reaches each through the best channel
it offers (`Instructions` in `agent/bot`):

1. **Claude: `_meta.systemPrompt`**, as above. claude-agent-acp 0.92 also advertises ACP's `systemPrompt`, but it
   rebuilds that field without the object options (`excludeDynamicSections`), so Claude stays on `_meta`.
2. **ACP `systemPrompt`** ([Client-Provided System Prompt](https://github.com/agentclientprotocol/agent-client-protocol/blob/main/docs/rfds/client-system-prompt.mdx),
   unstable): when an agent advertises `sessionCapabilities.systemPrompt`, the opening goes as
   `{"mode": "append", "content": [text]}` on `session/new` and `session/load` (the session's current snapshot). The
   agent keeps it at system level, across compaction.
3. **Every other agent: the first message.** A `<bot-profile>` block precedes the first prompt of a new session, and each retry until a prompt carrying it succeeds (the snapshot's `unprompted`, kept across a host restart).
   After a compaction the next prompt carries a fresh one that says it replaces any earlier profile, so the agent
   never holds two versions that disagree. The compaction is the agent's `compaction_update`, or, for an agent
   that never sent one, its `usage_update.used` falling below half of a previous value of at least 20k.

Measured 2026-10-18 at `initialize`:

| Agent | `systemPrompt` | `additionalDirectories` |
|---|---|---|
| claude-agent-acp 0.92.0 | Yes | Yes |
| codex-acp 2.2.2 | No | Yes |
| OpenCode 1.19 | No | No |
| Copilot CLI | No | No |

codex-acp reports compactions (`compaction_update`). Codex keeps the user's newest ~20k tokens of messages
through a compaction, so a first-message profile survives
short sessions and is the first thing dropped in long ones. In a real run past that budget (two 15k-token
messages, then `/compact`), Codex still knew the bot's name and the user's allergy afterwards, from its summary;
the re-sent profile makes that independent of what the summary keeps. codex-acp also reads
`_meta.jetbrains.air.customInstructions` on `session/new` as Codex developer instructions. It isn't used: it is one
vendor's extension, and it isn't re-sent on `session/load`.

Verified on a real host (2026-10-18): a Codex bot got its profile again on the first message after `/compact`
and still knew its name and the user's allergy; a Copilot bot reported no `store_memory` tool; a Claude bot
answered from its memory as before.

**File access.** The memory folders a bot can see go as `additionalDirectories` on every session request, when
the agent advertises the capability (M3). Agents that can't reach them read and write through `read_memory` and
`edit_memory`.

**The harness's own memory.** For every bot and every background job, Codync turns each harness's own memory off
where it can, so the bot has one memory and it is visible (`agent/own_memory.rs`, surveyed 2026-10-18). Where it
can, the user may keep it on for one bot ([Claude Code's own memory](#claude-codes-own-memory)):

| Harness | Its own memory | What Codync does |
|---|---|---|
| Claude Code | Auto memory, auto-dream | `autoMemoryEnabled: false`, `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1`, `autoDreamEnabled: false` |
| Copilot CLI | Copilot Memory, on for individual plans | `--excluded-tools=store_memory,vote_memory` |
| Qwen Code | Auto memory and its own dreaming, on | `.qwen/settings.json` in a bot's own workspace: `memory.enableManagedAutoMemory` and `enableManagedAutoDream` false. Nothing in a project folder, whose files are the user's |
| CodeBuddy | Auto memory | `CODEBUDDY_DISABLE_AUTO_MEMORY=1` |
| WorkBuddy | The same auto memory, in the CLI bundled with the app | `CODEBUDDY_DISABLE_AUTO_MEMORY=1` |
| Codex, Goose, Grok, oh-my-pi, Qoder, Cortex | Off unless the user turns it on | Nothing |
| Gemini CLI | Its system prompt has the model edit `~/.gemini/GEMINI.md` | Nothing: no switch short of replacing its system prompt or its home folder |
| Kilo Code | Memory tools, on per project | Nothing: the only per-process switch replaces the whole config |
| Kiro | Memory with a global setting | Nothing: no per-workspace key |
| Hermes Agent | `MEMORY.md` and `USER.md` in `~/.hermes/memories`, on, shared by every Hermes session | Nothing: its only switches are `memory.memory_enabled` and `user_profile_enabled` in the global `~/.hermes/config.yaml` |
| OpenCode, Cursor, Kimi, Amp, Cline, Droid, Devin, others | None found | Nothing |

### Compaction

The harness writes the summary, because it knows the work: files read, todos, tool results. The host decides
**when**, because it knows how full the context is, whether the user is present and whether the cache is warm.

Cache economics (Anthropic):

- a cache read costs 0.1× input, or less on newer models (0.05× on Opus and Sonnet 5.5);
- a 5-minute cache write costs 1.25×;
- a 1-hour cache write costs 2×.

Compacting an idle session before its cache expires beats a cold re-read once the context passes roughly 70–90k
tokens at 0.1× reads. Recompute this per model.

The 1-hour TTL applies to SDK turns only "on a Claude subscription within your plan's included usage". On
extra-usage credits or an API key it drops to 5 minutes. The host reads the TTL actually in use (the SDK's
`usage.cache_creation` separates 5-minute and 1-hour writes), or pins `promptCacheTtl: "1h"` for main lanes, which
doubles the write cost on API billing.

| Trigger | When | Why |
|---|---|---|
| **End of turn** | `used ≥ min(0.75 × size, 200k)` when a turn ends, leaving room for the summary (about 20k tokens) | The cache is hot, so reading the history costs ≈1/10. Nothing stalls mid-task. |
| **Idle** | 1-hour TTL: at TTL − 5 min when `used ≥ 100k`, main lane only. 5-minute TTL: no idle compaction. | Cheaper than a cold read on return. The threshold sits above break-even because compaction loses detail. |
| **Rollover** | Any of: (1) returning with a cold cache and `used ≥ 100k`; (2) away ≥ 8 hours with ≥ 3 compactions; (3) the loop guard fires | Summaries of summaries drift. A new session starts with a handoff. |
| **Safety net** | A single turn fills the window | The harness's own auto-compaction |

The `idleCompaction` setting needs no change. The harness never idle-compacts in SDK/ACP sessions, and setting it
to true "does not turn idle compaction on".

**Loop guard.** It allows at most 3 host-initiated compactions per lane per 24 hours. If post-compaction usage is
≥ 60% of the trigger, or two compactions happen within 5 turns, the host rolls over instead.

- Claude Code added a 3-attempt circuit breaker in 2.1.76 and thrash detection in 2.1.89, and still fixed
  "conversations being compacted over and over" in 2.1.301.
- One issue reported 211 compactions in a single session.

Group rooms and routines are never compacted early. Most are not revisited, so the cost isn't spent ahead
of time.

**Before a host-initiated compaction:**

1. dream;
2. render a new opening with the latest memory;
3. load it into the session;
4. compact with a keep-list.

This order puts the new opening into the very compaction that runs. Anthropic's docs say "the first compaction
switches to the current prompt".

How the keep-list is passed:

- **Claude:** `/compact <instructions>`, which the SDK turns into `custom_instructions`.
- **Codex:** `/compact` ignores trailing text, so the keep-list goes in as `compact_prompt` through `CODEX_CONFIG`.

The keep-list:

- **Verbatim:** the last user request, and every "don't / 不要".
- **Exact identifiers:** paths, URLs and ids (OpenClaw keeps "exact opaque identifiers").
- **Kept:** decisions with reasons, pending tasks with the next step, and who said what in rooms.
- **Left out:** identity and memory, because the host re-injects them.

The keep-list also lives in the opening, so harness-initiated compactions see it.

**After any compaction.** The signal is `compaction_update` completed, Claude's `compact_boundary`
`pre_tokens`/`post_tokens`, or a sudden drop in `usage_update.used` for harnesses that don't notify. Then:

1. Reset the ledger's delivered number to the opening's, and send one merged delta.
2. Re-send `<bot-profile>` to non-Claude harnesses. Do the same after every resume.
3. Store the summary as a transcript entry and show an "Earlier conversation summarized" card.
   - Codex's `compaction_update` carries no summary. For Codex the host builds the card from its own transcript:
     the boundary plus the latest turns.

**Rollover handoff.** It contains:

- the last compaction summary;
- the last 3–5 user turns verbatim;
- the open plan;
- a bookmark: the id of the old session's last message. The new session reads back from it with
  `search_history`'s `from` instead of guessing words, and pages on with the `next` each result page returns.

The user can edit it as a draft before it is sent (Amp's handoff works this way).

### Claude Code's own memory

| Bot | Auto memory | Auto-dream | Why |
|---|---|---|---|
| Every bot (default) | Off: `autoMemoryEnabled: false` + `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1` | Off: `autoDreamEnabled: false` | Only one memory, fully visible in Codync |
| Kept on by the user | Claude Code's own setting | Claude Code's own setting | The user's choice, warned first |

Until 2.18 a project bot kept auto memory on as the project's working memory. It was turned off on 2026-10-19 at
the owner's request, because memory is bot-based: its `user`/`feedback` notes overlapped Codync's person memory and
ran outside Codync's rules. A project bot keeps project knowledge in its own notes, and writes `CLAUDE.md` or
`AGENTS.md` only when the user asks.

The user can keep it on per bot, for example to share the auto-memory directory with their own Claude Code
sessions in the terminal. The switch sits on the memory screen with an info button on what it costs: the bot may
remember things twice, the two memories can disagree, and forgetting something in Codync doesn't reach Claude
Code's notes. Codync then shows the folder's `MEMORY.md` read-only, so it isn't unseen. The same switch covers
Copilot, CodeBuddy, WorkBuddy and (in a bot's own workspace) Qwen Code. Using it instead of Codync's memory is planned:
[harness memory](../plans/harness-memory.md).

The adapter passes `claudeCode.options.settings` and `.env` through untouched, so these settings reach the SDK.

### Storage

Host SQLite (`codync.db`), one transaction per change, every change stamped with `rev`. The `codync-mem` tables:

- `memory_files` and `memory_file_versions`: each file's layer, name, hash, size, mtime, description and tokens,
  and every version's full content, author and cause (30 days);
- `memory_facts`: who wrote each fact of a layer's `MEMORY.md`, when, and the message it was learned from;
- `memory_tombstones`: hashes of forgotten facts and of the messages they came from, and dreaming's own marks;
- `memory_dreams`, `memory_dream_lines` and `memory_dream_files`: the dream log;
- `memory_meta`: the schema version.

The cards of 2.18 to 2.20 (`memory_entries`, `memory_dream_changes`) move into the files once
(`codync_mem::legacy`), and their tables go. A card a newer one replaced, and a fact the checks refuse, stay out.

The transcript is the host's own `entries` table: it is the chat itself, which the apps sync by `rev` and which
changes in place while a reply streams. Search is a substring scan. When one bot's search passes a second, its
text moves to a plain column and an FTS5 trigram index with a CJK bigram column takes over: measured over 180k
messages, the scan took 0.50 s, ripgrep over the same text 0.013 s, and the trigram index 0.002 s after a 26 s
build.

### UI (all three clients)

- **Two sections**, "About you" and the bot's own: each `MEMORY.md` as facts with who wrote them, read-only.
- **A per-bot switch** for "yours".
- **Bot writes:** nothing in the chat (changed 2026-10-18 at the owner's request: memory lines and cards cluttered the
  chat). Every bot writes at once whatever its permission; the user reviews it in the memory settings, which are
  read-only since 2.21.0 (the owner's call: the user changes memory by telling the bot).
- **The dream log** (added green, archived red) with an info button saying what dreaming does.
- **The harness's own memory switch** in the bot's section, off by default, with an info button; kept on, Claude Code's `MEMORY.md` shows read-only.
- **"Shared" in place of the switch**, with an info button, where the harness keeps one memory every bot on it shares and Codync can't turn off (Gemini CLI, Hermes Agent: `harnessMemoryShared`).
- **Notes** under each layer's facts: name, description, size and date, an over-cap mark, and a view to read one.
- **In the chat**, only an editable handoff draft at rollover, folding to one line once sent; compactions and a
  kept handoff leave no line (2026-10-18, at the owner's request).

### Deliberately not built

Each of these has a trigger for adding it later.

| Not built | Add it when |
|---|---|
| Vectors | Keyword search and the bot's own rephrasing miss real cases; then a local model only, and a model change rebuilds the index |
| Knowledge graph | — |
| Project or team tiers | — |
| Multi-computer sync | — |
| Our own compaction summaries | Harness summaries prove insufficient |
| Per-reply memory citations | — |
| Engram | — |

## Engram

Engram fits its own goal: one coding agent's memory of its own work. The mismatch is shape, not quality.

| Engram | Codync needs |
|---|---|
| One database per bot; a `personal` scope stays inside it | A "yours" layer shared by all bots |
| Engram owns every write; Codync reads the pinned schema read-only | The gate, provenance with cascade delete, revisions, tombstones and dream undo, all inside Codync |
| The agent writes through Engram's tools, unseen and unlabeled | Bot writes follow its permission mode, name the bot, and show with undo |
| A separate process per bot; downloaded on first use; a lost acknowledgement means a write can't be retried safely | Memory that is as reliable as the host |
| Separate from `rev` | Memory and `rev` written in one transaction |

Everything Engram provides that we use, FTS5 search and a few tables, is a few SQLite tables in the host.
`host/src/chat/memory/engram/` (install, process, migration, backup, clear) disappears.

## Packaging

The memory system is built as a self-contained crate, `codync-mem`, so other ACP clients can use it later.

**A core that only decides (sans-IO).**

- The core has no processes, network, model calls or threads. Events and data go in; changes and commands come
  out.
- It contains:
  - the store;
  - the gate;
  - the dreaming rules;
  - the opening, delta, keep-list and handoff renderers;
  - the compaction-timing state machine with its loop guard;
  - the mapping from commands to ACP payloads (`_meta.systemPrompt` for Claude, first message for others,
    `compact_prompt` for Codex).
- It uses the **caller's SQLite connection**, keeps its own tables and migration version, and writes in the
  caller's transaction. Memory and `rev` therefore commit together.
- **Model work goes back to the driver.** The core returns a job (prompt plus answer format). The driver runs it
  on the user's own harness and never calls provider APIs. The core validates the answer deterministically before
  writing: user-authored verbatim evidence, minimum content, the ¼ guard, the operation limit, the format, and the
  `rev` precondition.

**Drivers.**

- **The Codync host** embeds the core. It is the full product: three-client UI, transcript in the host, and a gate
  that sees only what the user typed.
- **An ACP proxy (later)** lets any ACP client use the core without changes: `codync-mem acp -- <agent command>`.
  - It can inject deltas and observe `usage_update` and `compaction_update`.
  - It can't set the opening. The [proxy chains RFD](https://agentclientprotocol.com/rfds/proxy-chains) (Draft)
    says proxies "cannot directly modify an agent's system prompt".
  - That waits for the [Client-Provided System Prompt RFD](https://github.com/agentclientprotocol/agent-client-protocol/blob/main/docs/rfds/client-system-prompt.mdx),
    which adds `systemPrompt {mode, content}` to new, load, resume and fork. It is in the schema behind an
    unstable feature (PR #2361), and claude-agent-acp 0.92 advertises it.
  - It can't separate what the user typed from what the client injected, so its gate is weaker.
  - It has no UI; a CLI covers review and undo.
  - Dreaming and compaction run between turns on the user's quota.

ACP status as of 2026-10:

- Completed: Session Usage, and Session Compaction (2026-10-06).
- Draft: End-Turn Token Usage, Session Fork, Client-Provided System Prompt, Proxy Chains.

**Repository.**

- The crate lives at `host/crates/memory`, as a member of a `[workspace]` added to `host/Cargo.toml`.
- The build output stays in `host/target`, so `npm run dev` and packaging are unaffected.
- CI runs `cargo clippy --workspace`.
- **Codync is the source of truth.** Core and host will change together for months, since memory and `rev`
  commit in one transaction, so the crate is developed only here. No git submodule.
- **A read-only public mirror, `leepokai/codync-mem`,** gives outside developers a repository of their own.
  - On every push to `main` that touches `host/crates/memory`, CI runs
    `git subtree split --prefix=host/crates/memory` and pushes the result to the mirror's `main`.
  - The split is deterministic, so the push always fast-forwards as long as nobody commits to the mirror.
  - Issues are filed on the mirror. PRs go to Codync's `dev`: a bot answers a PR opened on the mirror with that
    pointer and closes it.
- **The crate builds on its own**, because the mirror is a plain copy of its folder:
  - its `Cargo.toml` inherits nothing from the workspace (lints, edition and version are written out);
  - its CI lives in `host/crates/memory/.github/workflows/`. GitHub reads only the root `.github`, so the CI is
    inert in Codync and runs once the folder is the mirror's root;
  - it re-exports `rusqlite` and documents the supported version. Callers pass their own connection, and one build
    can't link two `rusqlite` versions to `sqlite3`;
  - it has its own README and LICENSE (Apache-2.0).
- The crate has its own 0.x versions, separate from Codync's.
- The mirror is open, marked 0.x with an unstable API, and 0.1.0 is on crates.io (2026-10-18; the core only, the
  proxy comes later). It ships with the Codync release that changes it: a pull request into `main` that changes
  the crate must raise its `version`, and the push to `main` publishes it with trusted publishing
  ([codync-mem releases](../guides/development.md#codync-mem-releases)).
- **Flip later.** When outside users send PRs, the API stops changing with host changes for a few months, or the
  proxy gains users:
  - stop the sync and develop in the mirror;
  - switch Codync to the crates.io release and delete `host/crates/memory`.

  The mirror already holds the history, so nothing moves.

## Verification status

What the research answered:

1. **Does compaction pick up the latest loaded opening? Yes.**
   - Docs: "The new text takes effect after that compaction, or in a new session."
   - In the SDK binary, the recorded prompt is a hidden `prompt_snapshot` attachment. After a full compaction the
     message list is rebuilt without it, so the next request renders from the current options. This part is
     inferred: no explicit delete was found.
   - Resume reads the record and doesn't re-record.
   - Hence: render, load, then compact.
2. **Can the settings be set through ACP? Yes.**
   - `autoMemoryEnabled`, `autoDreamEnabled` and `idleCompaction` go through `claudeCode.options.settings`, which
     the adapter passes on untouched.
   - `excludeDynamicSections` and `snapshot` go on the `_meta.systemPrompt` object.
3. **What does Codex report?**
   - `codex-acp` reports `usage_update{used,size}` (from `thread/tokenUsage/updated`) and per-turn cached tokens.
   - It sends `compaction_update` in_progress/completed when the client declares `session.compaction`, which
     Codync does. The update carries no summary and no token counts.
   - Its `/compact` takes no instructions.
4. **Is recording on for Codync's sessions? Yes.**
   - Default-on since Claude Code 2.1.265; the feature-flag dependency ended in 2.1.268.
   - In 2.1.300 only `systemPromptSnapshot: false` or `CLAUDE_CODE_SIMPLE` turn it off.
   - Installed versions: Claude Code 2.1.301 and adapter 0.92.0 (SDK 0.3.300).

Still unknown:

- **Partial compaction:** how "Summarize from here" treats the recorded prompt.
- **2.1.301:** the installed binary wasn't searched, only the 2.1.300 binary bundled in the SDK.
- **Codex's auto-compact threshold:** about 90% of the window, from a secondary source only.

The real flow, measured on a real host with Claude (Opus, 1M window; 2026-10-18):

- **Cache:** after the first turn, a plain turn read about 97k tokens from cache and wrote about 250.
- **Memory change mid-session:** a card saved between turns reached the bot in the next message. That turn
  wrote 264 tokens to cache, so there was no reload.
- **Thread:** a thread forked from that chat knew a card saved just before it and one saved earlier.
- **Host compaction:** one message pushed the chat past the 200k trigger. Right after that turn the host ran
  `/compact` with the keep-list. It first reloaded the session, because memory had changed since it opened.
  Afterwards the context was down to 13k, the chat line carried Claude's summary, and the next turn still knew
  the code word from the long message and the new card.
- **Long-form notes:** given the folders through `additionalDirectories`, the bot wrote `mochi.md` with its own
  Write tool, frontmatter and description included. The scan credited it to the bot from the tool call's
  `locations`, the chat showed "Created mochi.md", and in the next turn the bot read the note to answer.

## Evaluation

**Property tests** in the crate:

- no memory without a user-authored verbatim span;
- pasted and quoted spans never count;
- acknowledgement-only quotes are rejected;
- the ¼ guard and the 10-operation limit;
- tombstones never resurrect;
- cascade on message delete;
- the temporal flip;
- the `rev` precondition;
- the loop guard.

**Scripted regression.** About 50 zh-Hant and English conversations covering:

- LongMemEval's five abilities: extraction, multi-session reasoning, temporal reasoning, knowledge update and
  abstention;
- MemoryAgentBench's selective forgetting.

Validate the judge adversarially first: a LoCoMo audit found a judge accepting up to 63% of intentionally wrong
answers.

Built as `tools/memory-eval/` (52 cases: also long-form notes, short CJK queries and planted instructions). It
grades by keywords, not a model, so a wrong answer can't argue its way to a pass; keywords that missed right
answers were widened after reading every reply. Run on Claude (Opus, 2026-10-18), one fresh bot per case:

| | Passed | Fixed by |
|---|---|---|
| A pilot of 9 cases | 8/9 | a reply refused by the user's Claude Code auto-mode classifier for naming their daughter: Codync's chat and memory tools are now allowed in the Claude options |
| The full run | 41/52 | |
| Right answers the grader missed (spacing in "3 月 14 日", 羽毛球, 燕麥奶, "沒跟我說過", a reply naming the planted bank to warn about it) | +8 | the grader |
| An itinerary kept as entries, not a note; a forgotten fact read back from history | +2 | the memory instructions now name lists, itineraries and histories for notes, and say forgetting covers history |
| Left | 51/52 | ex-05: the user's own `~/.claude/CLAUDE.md` made the bot answer with an HTML file |

Decisions from it: the caps stay (no case reached one); no FTS5 or vectors (every recall miss was the answer
format, never search); dreaming unchanged.

It also showed a bot inheriting the user's own Claude Code setup: `~/.claude/CLAUDE.md`, its permission mode, and
the claude.ai connectors (one bot looked through the user's Gmail and Calendar unasked). Isolating it
(`settingSources: ["project"]` for bots in their own workspace) changes what existing bots can do, so it waits
for the owner's decision.

**Compaction probes** after each compaction, Factory-style:

- recall;
- artifacts (paths, ids);
- continuation;
- decisions.

**Prompt cache.** The host logs each turn's cache reads and writes (`turn usage` in `host.log`).
`tools/memory-eval/cache.py` measures the hit rate from Claude Code's and Codex's own session logs; on 2026-10-18:

| | Conversations | Calls | In a conversation | Every call |
|---|---|---|---|---|
| The owner's Claude bots (`~/.codync`, `~/.codync-dev`) | 27 | 229 | **97.6%** | 83.7% |
| The owner's Codex bots | 7 | 24 | 96.7% | 81.4% |
| The evaluation run (Claude) | 173 | 472 | 98.1% | 71.1% |

The owner's Cursor, oh-my-pi and custom bots aren't measured: the script reads only those two logs. Once the host
from this branch runs, its `turn usage` lines cover every harness whose adapter reports cached tokens.

- **The owner's cache lives an hour.** Their Claude bots run on a Claude subscription, so every cache write was
  a 1-hour entry (100% of `usage.cache_creation`). On extra-usage credits or an API key it is five minutes, and
  the cold calls would start five minutes after the last one. Codex keeps OpenAI's own cache life, not measured.
- **In a conversation** is the share of input read from cache, leaving out each conversation's first call and
  calls more than an hour after the previous one. Per call it is 97.2%, and the median conversation 99.2%.
- **Every call** adds those back. Many conversations are only 1–3 calls, so per conversation it averages 61.4%.
- **Cold calls on the owner's bots:**
  - 25 first calls;
  - 22 after the cache expired;
  - 4 mid-session misses, each reading about 15k of 46k. All of them date from 2026-09-30 to 10-10, before the
    cache-safe opening. The evaluation had none.
- **Cost:** Claude Code writes every cache entry for an hour, at 2× input. Overall, input cost 36.9% of the same
  calls uncached. A one-call conversation costs more than no cache would (157%): it writes the cache and never
  reads it back. Codex (OpenAI) caches on its own and charges nothing to write.

**Product metrics** (PostHog counts only, never content):

- the 7-day regret rate (memories the user edits or deletes);
- the dream-undo rate;
- compactions per session;
- rollovers;
- cache hit ratio before and after compaction;
- "wrong memory" taps.

## Rollout

Each milestone ships as its own minor release, with an e2e pass on all three apps.

- **M0. Cards, dreaming, the move from Engram (2.18.0, built).**
  - `host/crates/memory` in the standalone shape from [Packaging](#packaging); the tables; the move from Engram
    and Markdown memory in one pass.
  - The gate with pasted-span tagging in all three composers, the source policy and the sensitive-data rules.
  - The bot's memory tools: direct writes with undo in `Auto`, confirm cards in `Ask`; the shared "yours" layer.
  - Claude Code's auto memory off for personal bots, auto-dream off for all.
  - Dreaming: proposal, audit, evidence ids, the `rev` precondition, limits, headroom, the time review, the log
    with undo. Automatic naming at turn end.
  - Two layers on the memory screens of all three clients; `minApp` 2.18.0.
- **M1. Instructions for every harness (built).**
  - ACP `systemPrompt` where advertised; the first-message profile again after a compaction elsewhere, the
    compaction inferred from usage when the agent doesn't report it.
  - Each harness's own memory turned off for personal bots where it can be.
- **M2. Cache-safe opening and compaction (built).**
  - Remove the reload; send the `_meta.systemPrompt` object with `excludeDynamicSections`; fork re-sends it.
  - The ledger and deltas.
  - Usage and the TTL; the three triggers with the loop guard and the before/after steps.
  - The keep-list: Claude through `/compact`, Codex through `compact_prompt`.
  - The summary card, the editable handoff and its history bookmark.
  - Cache tokens logged per turn and measured on real bots ([prompt cache](#evaluation)).
- **M3. Long-form memory (built).**
  - The folders, the naming and cap rules, `memory_files` and `memory_file_versions`.
  - The turn-end scan, versions, checks and restore; compare-and-swap host writes.
  - The index in the opening; `read_memory`, `edit_memory`, and `search_memory` over files;
    `additionalDirectories`.
  - Dreaming appends and moves detail out of full card layers.
  - The files, versions and "Updated" lines in all three clients.
- **M4. Evaluation and tuning (built).** The scripted regression below, plus questions answered only by a
  long-form file, short CJK queries and planted instructions; tune the caps and dreaming from it, and decide FTS5
  and vectors by their triggers. The runner and its 52 cases are in `tools/memory-eval/`; the results are under
  [Evaluation](#evaluation).
- **M5. The public mirror.** `leepokai/codync-mem`, synced on every release, as under [Packaging](#packaging).
  The sync (`.github/workflows/memory-crate.yml`) pushes with the `CODYNC_MEM_DEPLOY_KEY` secret, a write deploy
  key on the mirror only, then publishes new versions to crates.io; the mirror's own CI and pull-request redirect
  live in `host/crates/memory/.github/`. Opened 2026-10-18 with the 2.18.0 split, and its CI passed on Linux, macOS
  and Windows; 0.1.0 was published by hand the same day.

## Sources

**Products**

- ChatGPT: [memory and dreaming](https://openai.com/index/chatgpt-memory-dreaming/), [Memory FAQ](https://help.openai.com/en/articles/8590148-memory-faq), [Projects](https://help.openai.com/en/articles/10169521-projects-in-chatgpt)
- Claude: [chat search and memory](https://support.claude.com/en/articles/11817273-use-claude-s-chat-search-and-memory-to-build-on-previous-context)
- Claude Code docs: [memory](https://code.claude.com/docs/en/memory), [context window](https://code.claude.com/docs/en/context-window), [prompt caching](https://code.claude.com/docs/en/prompt-caching), [hooks](https://code.claude.com/docs/en/hooks), [settings](https://code.claude.com/docs/en/settings-reference), [modifying system prompts](https://code.claude.com/docs/en/agent-sdk/modifying-system-prompts), [changelog](https://raw.githubusercontent.com/anthropics/claude-code/main/CHANGELOG.md)
- Claude Code issues: [auto-dream #38493](https://github.com/anthropics/claude-code/issues/38493)
- Anthropic API: [prompt caching](https://platform.claude.com/docs/en/build-with-claude/prompt-caching), [compaction](https://platform.claude.com/docs/en/build-with-claude/compaction-on-demand), [context editing](https://platform.claude.com/docs/en/build-with-claude/context-editing), [memory tool](https://platform.claude.com/docs/en/agents-and-tools/tool-use/memory-tool)
- Grok: [designing Grok Bot](https://x.ai/news/designing-grok-bot), [Team Bots](https://x.ai/news/team-bots), [Grok Build memory](https://x.ai/news/grok-build-memory)
- Codex: [memories](https://learn.chatgpt.com/docs/customization/memories), [`compact.rs`](https://raw.githubusercontent.com/openai/codex/main/codex-rs/core/src/compact.rs), [`compact_remote_v2.rs`](https://raw.githubusercontent.com/openai/codex/main/codex-rs/core/src/compact_remote_v2.rs)
- Coding agents: [Amp Handoff](https://ampcode.com/news/handoff), [Factory: evaluating compression](https://factory.com/news/evaluating-compression), [Cursor: dynamic context discovery](https://cursor.com/en-US/blog/dynamic-context-discovery)
- OpenClaw: [dreaming](https://docs.openclaw.ai/concepts/dreaming), [compaction](https://docs.openclaw.ai/concepts/compaction)
- Hermes: [memory](https://hermes-agent.nousresearch.com/docs/user-guide/features/memory), [#5563](https://github.com/NousResearch/hermes-agent/issues/5563)
- Letta: [memory blocks](https://docs.letta.com/guides/agents/memory-blocks), [sleep-time compute](https://www.letta.com/blog/sleep-time-compute), [benchmarking agent memory](https://www.letta.com/blog/benchmarking-ai-agent-memory)
- Mem0: [token-efficient memory](https://mem0.ai/blog/mem0-the-token-efficient-memory-algorithm)
- Zep/Graphiti: [Graphiti](https://github.com/getzep/graphiti), [Zep](https://arxiv.org/abs/2501.13956)

**Research**

- Benchmarks: [LongMemEval](https://arxiv.org/abs/2410.10813), [MemoryAgentBench](https://arxiv.org/abs/2507.05257), [LoCoMo audit](https://penfieldlabs.substack.com/p/we-audited-locomo-64-of-the-answer)
- Memory systems: [A-MEM](https://arxiv.org/abs/2502.12110), [Context Rot](https://www.trychroma.com/research/context-rot)
- Attacks: [MINJA](https://arxiv.org/abs/2503.03704), [AgentPoison](https://arxiv.org/abs/2407.12784), [multi-agent spread](https://arxiv.org/abs/2407.07791), [Gemini delayed tool invocation](https://embracethered.com/blog/posts/2025/gemini-memory-persistence-prompt-injection/), [Microsoft: AI recommendation poisoning](https://www.microsoft.com/en-us/security/blog/2026/02/10/ai-recommendation-poisoning/)
- Defenses: [CaMeL](https://arxiv.org/abs/2503.18813), [design patterns for securing LLM agents](https://arxiv.org/html/2506.08837)

**Community**

- [r/ChatGPT project-only memory leak](https://old.reddit.com/r/ChatGPT/comments/1rdei2p/)
- [r/ChatGPT turning off memory](https://old.reddit.com/r/ChatGPT/comments/1ve7jch/)
- [HN: Claude Memory](https://news.ycombinator.com/item?id=45684134)

**ACP**

- [RFD updates](https://agentclientprotocol.com/rfds/updates), [proxy chains](https://agentclientprotocol.com/rfds/proxy-chains), [client-provided system prompt](https://agentclientprotocol.com/rfds/client-system-prompt)
- [`codex-acp`](https://github.com/agentclientprotocol/codex-acp), [`agent-client-protocol` 2.0 proxies](https://docs.rs/agent-client-protocol/2.0.0/agent_client_protocol/concepts/proxies/index.html)

**Local**

- `@agentclientprotocol/claude-agent-acp` 0.92.0 (`acp-agent.js`, `context-compaction.js`)
- `@anthropic-ai/claude-agent-sdk` 0.3.300 (`sdk.d.ts`, bundled Claude Code 2.1.300)
