# Bot memory research: files, databases and harnesses

The third research pass behind [bot memory and compaction](bot-memory.md), done 2026-10-17 and 2026-10-18. The
first two passes are summarized in that doc's [Patterns explored](bot-memory.md#patterns-explored). This one asks:

- Should memory use embeddings, SQLite, the file system, or a mix, and what goes where?
- How do open-source agents that mix Markdown files with a database actually do it?
- What does each harness do with Codync's instructions when it compacts, and what does ACP standardize?

Claims come from reading source unless marked **docs** (stated in a project's docs) or **inferred**. Issue
numbers are the projects' GitHub issues. The conclusions in [What this means for Codync](#what-this-means-for-codync)
were adopted on 2026-10-18; the design itself is in [bot memory and compaction](bot-memory.md).

## Short answers

| Question | Answer |
|---|---|
| Embeddings? | Not for memory about one person. Add them only when keyword search plus the model's own rephrasing misses real cases. |
| SQLite? | Yes, for what needs guarantees: provenance, the change journal, the transcript and the search index. |
| File system? | Yes, for long prose the agent reads and edits with its own tools. Every system studied keeps an always-loaded small layer and an on-demand larger one. |
| Mix? | Each kind of data has exactly one home. Files hold prose; the database holds what the model must not be able to rewrite. |

## Embeddings

| Source | Finding | Caveat |
|---|---|---|
| Claude Code ([interview](https://newsletter.pragmaticengineer.com/p/building-claude-code-with-boris-cherny?open=false)) | Started with RAG on a local vector database, then switched to agentic search (glob and grep driven by the model): it worked better and nothing goes stale. | The comparison was mostly internal impressions ("vibes"). |
| Cursor ([post](https://cursor.com/en-US/blog/semsearch)) | Trained its own embedding model. Semantic search plus grep raised answer accuracy 12.5% on average (6.5%–23.5% by model). The combination works best. | Code search over large repositories, not personal memory. |
| Letta ([post](https://www.letta.com/blog/benchmarking-ai-agent-memory)) | Conversation history stored as files, agent with file tools: 74.0% on LoCoMo, above the 68.5% Mem0 reports for itself. | One of the tools, `search_files`, is semantic. A plain full-context baseline scores about 73%. LoCoMo doesn't test knowledge updates. |
| LongMemEval ([paper](https://arxiv.org/abs/2410.10813)) | Chat assistants and long-context models lose about 30% accuracy on memory across sessions. | The hard abilities are knowledge updates and temporal reasoning, not lookup. |
| Anthropic memory tool ([docs](https://platform.claude.com/docs/en/agents-and-tools/tool-use/memory-tool)) | File operations (`view`, `create`, `str_replace`, …) under `/memories`; the client decides the storage. | — |
| claude-mem, OpenClaw, memsearch | All three keep a vector copy. claude-mem's Chroma copy is its largest bug source (#3216: 759 processes, ~71 GB; #3940: corruption; #4805: deletes don't reach it). OpenClaw's default embedder calls OpenAI's API. | — |

Codync can't call provider APIs with agent credentials, so embeddings would need a local model on three platforms,
re-embedding on every model change, and a second index to keep consistent.

## Systems studied

### Grok Bot 0.18

Studied from an unofficial reconstruction of the desktop app's source; the current app no longer ships its memory
logic locally.

- **Storage:** files only, no database, no embeddings. Each bot has a folder with `profile.md` and
  `log/YYYY-MM.md`, one `- (YYYY-MM-DD) fact` per line.
- **Prompt:** profile facts (up to 100) and recent log facts (30, within 4,000 characters, ranked by importance with
  a 30-day half-life). Older facts: "N more log facts on disk, grep the log/ folder". The memory section is frozen
  per compaction epoch.
- **Writers:** one tool, `update_state` (`write` with a tier, `forget` by exact text). The prompt asks the agent to
  prefer it over editing the files, which it may read and grep with its own tools.
- **Metadata as files** in `.dreaming/`: empty files named by content hash mark each fact's origin (`explicit`,
  `synthesis`, `legacy`) and tombstones. Dreaming may never change `explicit` facts and never recreates a
  tombstoned one.
- **Dreaming:** 15 s after a turn, hourly poll, daily temporal review, up to 512 facts as input. A SHA-256
  fingerprint of all files is taken first; if it changed when the plan returns, the run is discarded.
- **Sharing:** user memory is one shard folder per bot, "so every file has a single writer"; the newest fact wins on
  conflict. Project memory is opt-in per project, also sharded per bot.

### OpenClaw (MIT)

- **Storage:** Markdown in `~/.openclaw/workspace`: `USER.md` (profile, with `<!-- observed: … | status: … -->`
  comments), `MEMORY.md` (curated), `memory/YYYY-MM-DD[-slug].md` (daily notes), `DREAMS.md` (a diary for humans).
- **Database:** a per-agent SQLite with `memory_index_sources` (path, SHA-256, mtime, size), chunks, FTS5 and
  `vec0` tables. It also holds what files must not: provenance (`origin_class`: owner, agent, untrusted, system),
  which "the model cannot write through prose", dreaming state and the pre-images of `MEMORY.md` rewrites.
- **Sync:** file watcher (1.5 s debounce), a full recovery scan every 5 minutes, polling when watching fails
  (**docs**). A file whose hash changes during indexing is retried. A full reindex builds a new database and swaps it.
- **Prompt:** `USER.md` (4,000-character cap) and `MEMORY.md` (20,000 per file, 60,000 total) are injected and
  truncated with a "read the file" notice. Daily notes come through `memory_search` / `memory_get`. On the Codex
  harness `MEMORY.md` isn't pasted; a note points to the tools (**docs**).
- **Writers:** the agent edits the files with its normal tools. Dreaming (nightly) is the only gated writer of
  `MEMORY.md`: it must keep at least 75% of prior entries, cite sources and stay in budget, re-checks the hash before
  an atomic rename, and appends on conflict. The pre-compaction flush only appends to today's note.
- **Gaps:** no secret or injection check on agent edits (**inferred**). Writers are serialized only inside one
  process: #40001 (cron sessions overwrite shared files), #117741 (concurrent flushes lose an append), #90354
  (unbounded flush appends). Flush misfires: #12590, #45608.
- **Search:** BM25 plus vectors, recency decay, MMR. CJK is broken in several open issues: #170997 (2-character
  queries return nothing under trigram), #110500 (BM25 score always 0), #140736.
- **QMD**, a separate Markdown search engine, was its second backend until PR #120936 removed it (about 19.5k lines
  of upkeep). QMD itself is worth reading for change detection: mtime plus size, then SHA-256, a "racily clean"
  window that distrusts files modified within 2 s, and a guard so a missing root isn't read as "everything deleted".

### Hermes Agent (MIT)

- **Storage:** two files, `~/.hermes/memories/MEMORY.md` (2,200 characters) and `USER.md` (1,375), entries
  separated by `§`. Transcripts in SQLite with FTS5. Skills as `SKILL.md` folders with three levels of disclosure.
- **Prompt:** both files are rendered once at load with a usage header; mid-session writes reach the disk, not the
  prompt, until compaction or `/new`. A skill added mid-session is announced on the next user message (**docs**).
  The point is the prefix cache (**docs**).
- **Writers:** a `memory` tool with `add`, `patch`, `remove`. `replace` was withdrawn because it wiped facts packed
  next to the target (#117952). A background review may only `add`; patches and removals wait for approval
  (#105921). Writes lock, re-read the file, then write through a temp file and rename; a file holding text Hermes
  didn't write is refused and backed up (#26045). Injection scan on write and on load (a flagged entry becomes a
  `[BLOCKED…]` placeholder in the prompt).
- **Gaps:** no history or undo (#76883). #76035: a hallucinated full rewrite destroyed `USER.md`. #42405: a retry
  loop at capacity.
- **CJK:** a custom FTS5 tokenizer that splits CJK into overlapping bigrams, because trigram can't match 1–2
  character terms.

### Claude Code auto memory (proprietary, from docs and issues)

- **Storage:** files only: `~/.claude/projects/<project>/memory/MEMORY.md` (an index, one line per memory) plus one
  topic file per memory with `type` frontmatter (**docs**).
- **Prompt:** the first 200 lines or 25 KB of `MEMORY.md` load at session start; topic files are read on demand.
  Memory sits in a layer that changes only at session start, `/clear` or `/compact` (**docs**).
- **Writers:** Claude with Write and Edit; the user via `/memory`. No versioning or undo (**inferred**).
- **Problems:** #47959 (auto-dream deleted 23 files in a day), #99403 and #82056 (silent truncation of the index;
  a session can't tell whether it loaded whole), #38493 (auto-dream keeps no log).

### claude-mem (Apache-2.0)

- SQLite is the source, Chroma a vector copy, Markdown only an optional generated output.
- A session-start hook injects a compact index of the last 50 observations; details come through
  `search` → `timeline` → `get_observations`.
- Only a background process writes (an observer model over hook events); the agent itself doesn't.
- CJK can't be split by `unicode61`, so queries fall back to `LIKE` per script run (#3801).

### Basic Memory (AGPL-3.0)

- **Storage:** one Markdown note per file with YAML frontmatter; facts as `- [category] text`, links as `[[Target]]`.
- **Database:** per-file checksums and mtimes, accepted versions and write status, a change journal (operation,
  paths, checksum, no content), and a rebuildable search index.
- **Sync:** a watcher (1 s debounce) and a startup scan (mtime and size within 10 ms reuse the checksum, otherwise
  SHA-256). Renames are detected by checksum. Writes are compare-and-swap: when the bytes on disk match neither the
  accepted nor the expected version, it refuses and marks the note `external_change_detected`.
- **Problems:** #1256 (DB-first writes lost a note before its file existed), #124 (no undo: the journal has no
  content), #1171 and #528 (its own frontmatter rewrites duplicated blocks), #1275 and #1281 (macOS NFC/NFD and
  case-only renames made duplicate notes), #1294 (`unicode61` broke CJK; fixed with a bigram column).

### memsearch (MIT)

- Daily Markdown journals are the source; a Milvus index is a rebuildable cache.
- Chunk ids include line numbers, so inserting text re-embeds everything below it (#786).
- The stop hook appends a summary with no lock; optional maintenance replaces `PROJECT.md`/`USER.md` wholesale with
  no backup.
- Many issues about orphaned watcher processes (#692, #708, #658, #600).

### Letta (Apache-2.0)

- **V1 (retired) files:** `open_files` (5 open at most, least recently used closed), `grep_files`,
  `semantic_search_files`. Open file contents render inside the system prompt, so opening a file changes the
  prompt (**inferred**).
- **letta-code MemFS (current):** memory is a git repository of Markdown files. Files under `system/` render in full;
  everything else appears only as a tree of paths, read on demand. Every file needs a frontmatter `description`. A
  pre-commit hook rejects a missing description, edits to `read_only` files, and files or totals over the limits in
  `.memfs.config.json`; only a human may change the limits. Background workers edit a private worktree that gets
  merged.

### Obelisk (AGPL-3.0)

- Indexes ten coding agents' local session logs into one SQLite with full-text search; queries run there, and its
  query scripts can't touch the file system (`fs.*` is denied by default).
- The agent writes its own query scripts (JavaScript in a `vm` sandbox that can be escaped).
- Memories are Markdown files registered in the database; recall searches an English summary and the path, so other
  languages need translating first.
- Its context-window plugin for one harness hands off to a new session with a pointer back into the old one; this is
  where the rollover bookmark in [bot memory](bot-memory.md) came from.

## Lessons across systems

1. **Two layers everywhere.** A small layer always in the prompt (Grok Bot's profile, OpenClaw's `USER.md`/`MEMORY.md`,
   Claude Code's index, letta-code's `system/`) and a larger one read on demand by path.
2. **The database holds what prose can't be trusted with:** provenance, pre-images, versions, tombstones, the index.
   OpenClaw and Grok Bot both keep provenance out of the model's reach.
3. **Never rewrite whole entries or files unattended.** Hermes withdrew `replace`; Claude Code's auto-dream deleted
   files; Mem0 went add-only. OpenClaw rejects rewrites that drop more than a quarter of entries.
4. **Journal content, not checksums.** Basic Memory and Hermes have no undo because they don't keep the old text.
5. **Concurrent writers need compare-and-swap.** Hermes, OpenClaw, Basic Memory and Grok Bot all re-check a hash right
   before writing; the ones that don't lose appends.
6. **Detect changes by hash.** mtime and size as the fast path, SHA-256 to decide, a short racy window, and a guard
   against a missing folder.
7. **Silent truncation is the most resented failure.** Flag an index over its cap; never cut it at load.
8. **CJK is broken by default.** `unicode61` treats a CJK run as one token and trigram misses 1–2 character words.
   Hermes and Basic Memory fixed it with bigrams; claude-mem falls back to `LIKE`.
9. **Freeze the prompt.** Hermes, Claude Code and Grok Bot change memory in the prompt only at a boundary
   (compaction, new session); OpenClaw appends refreshed sections after the user turn.
10. **File names are data.** Normalize Unicode and case, or one note becomes two.

## Harnesses: instructions and compaction

### What Codync does today

- Claude gets the bot's instructions and memory cards as its system prompt (`_meta.systemPrompt`).
- Every other harness gets them as a `<bot-profile>` block in the first message of a new session
  (`host/src/agent/bot/turn.rs`). Compaction bumps the epoch and asks dreaming to run, but the profile isn't sent
  again. If a harness drops that first message when it compacts, the bot loses its instructions and memory cards,
  and memory written meanwhile reaches it only in a new session. M1 adds the re-send.

### Codex

- Local compaction keeps the user's messages from newest to oldest within `COMPACT_USER_MESSAGE_MAX_TOKENS = 20_000`,
  then appends the summary. Its own initial context (developer instructions, `AGENTS.md`, environment) is rebuilt
  from the session and re-inserted (`codex-rs/core/src/compact.rs`).
- So the first message survives while the user's messages stay under about 20k tokens, and is the first to go after.
  A blind re-send would put two profiles, possibly with different memory, in the history.
- Codex has `developer_instructions` ("Developer instructions override injected as a separate message") and
  codex-acp merges a `CODEX_CONFIG` JSON into the session config. A profile delivered there should survive
  compaction on its own (**inferred**). `CODEX_CONFIG` is set when the agent process starts, so per-session values
  need checking. The remote compaction path wasn't read.
- codex-acp also reads `_meta.jetbrains.air.customInstructions` on `session/new` as developer instructions
  (`src/AirExtension.ts`), but not on `session/load`.
- Measured with codex-acp 2.2.2 (2026-10-18): a profile, two ~15k-token user messages, `/compact` (context use
  55.7k → 32.8k), then "what is your name and what is the user allergic to?". With the profile in the first message
  and with it in `customInstructions`, Codex answered both correctly, so its summary carried the profile this time.

### Other harnesses

Not verified. Each compacts differently, and each limits file access outside the working directory differently
(Claude asks for permission, Codex's sandbox may refuse writes, Gemini CLI limits its file tools to the workspace,
OpenCode has an external-directory permission). Some have their own memory feature (Gemini CLI's `save_memory`
writes a global `GEMINI.md`); Codync turns off only Claude Code's.

### ACP

- **`additionalDirectories` is stable** on `session/new`: "Additional workspace roots for this session … expand the
  session's filesystem scope without changing `cwd`." claude-agent-acp and codex-acp both use it. Codync doesn't send
  it yet.
- **`systemPrompt` is unstable.** The RFD [Client-Provided System Prompt](https://github.com/agentclientprotocol/agent-client-protocol/blob/main/docs/rfds/client-system-prompt.mdx)
  landed in the schema (PR #2361) behind `unstable_session_system_prompt`:
  - an agent advertises `sessionCapabilities.systemPrompt` at `initialize`; clients send nothing otherwise;
  - `systemPrompt: { mode, content }` on `session/new`, re-sent on `load`, `resume` and `fork`;
  - `append` is required, `override` is a separate opt-in;
  - agents SHOULD place it at system level (Codex: developer instructions), and MAY defer a changed prompt until
    they next rebuild it, such as after compaction, to keep caches.
- The RFD names Codync's two workarounds as the status quo: claude-agent-acp's `_meta.systemPrompt` (which dropped
  `excludeDynamicSections` for months, #581) and instructions injected into user messages. claude-agent-acp has an
  implementation (`src/client-system-prompt.ts`); none was found in codex-acp.

## Codync measurements

- **Transcript search:** the `search_history` scan took 0.59 s over 180k synthetic messages of one bot (a 140 MB
  database). The largest bot on the dev host has 36 messages.
- **Real Claude bots** (sandbox host, 2026-10-17):
  - Claude ran ToolSearch before every memory tool, because Claude Code defers MCP tools; its built-in Read needs no
    such step.
  - Given a memory's `source`, it still searched by keyword; asked what was said before, it read back without a query.
  - A second bot knew the shared fact and not the first bot's own memory.

## What this means for Codync

Adopted on 2026-10-18 as milestones M1–M5 of [bot memory and compaction](bot-memory.md#rollout).

**Three layers, one home each:**

| Layer | Home | In the opening | Written by |
|---|---|---|---|
| Memory cards | `codync.db` | In full, within the caps | User, bots (by permission), dreaming |
| Long-form memory | Markdown files | Index only: path and description | User, bots with their own file tools, dreaming |
| Transcript | `codync.db` | Never | The host |

The journal (every version's content and author), provenance, tombstones and the search index stay in `codync.db`
for both cards and files.

**Paths:**

```
<data>/                          CODYNC_HOME, normally ~/.codync (dev: ~/.codync-dev)
├── codync.db
├── shared-memory/*.md           long-form memory about the user, for bots that use the shared layer
└── bots/<bot-id>/bot-memory/*.md  one bot's own long-form memory
```

- Not `bots/<id>/memory/`: the move from the old memory deletes that folder on every start.
- Project bots keep their files here too, outside the project repository.
- File names are lowercase ASCII slugs (`people.md`); the title in the file can be any language. One level, no
  subfolders.
- Every file starts with a frontmatter `description`, which becomes its index line.
- Caps (initial): 20 files per folder, 2,000 tokens per file. An index over its cap is flagged, never cut.

**Writes and checks:**

- At turn end the host scans both folders (mtime and size, then SHA-256), journals each changed file as a new
  version, and runs the secret and instruction checks. A bad edit is reverted to its journaled pre-image without
  undoing a concurrent edit by the user; an edit that drops more than about a quarter of a file is flagged.
- The host writes files only for dreaming and for edits from the apps, through compare-and-swap and an atomic
  rename; it never reformats them.
- Dreaming moves details from a full card layer into the matching file instead of archiving them.

**Search:** keep the substring scan. An FTS5 index, when needed, uses trigram plus a bigram column or a substring
fallback for 1–2 character terms. Embeddings wait for the trigger above, with a local model only.

**Harnesses:**

1. Instructions: ACP `systemPrompt` (append) when the agent advertises it; otherwise `_meta.systemPrompt` for Claude;
   otherwise the first-message `<bot-profile>`, plus a replacing update after a detected compaction.
2. File access: `additionalDirectories` with both folders. Harnesses that can't reach them get `read_memory` and
   `edit_memory` on Codync's memory MCP server.
3. Built-in memory: survey each harness and turn its own memory off for personal bots, as for Claude Code.

**Decided (2026-10-18):**

- Bots edit memory files with their own tools, checked at turn end; `edit_memory` is there for harnesses that
  can't reach the folders.
- The folders are `shared-memory/` and `bots/<id>/bot-memory/`.
- Order: instructions for every harness (M1), then the cache-safe opening and compaction (M2), then the files (M3).

**Verified at `initialize` (2026-10-18):** claude-agent-acp 0.92.0 advertises `systemPrompt` and
`additionalDirectories`; codex-acp 2.2.2 only `additionalDirectories`; OpenCode 1.19 and Copilot CLI neither.
- Each harness's own memory feature and how to turn it off.
