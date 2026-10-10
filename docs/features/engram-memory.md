# Engram memory integration

Codync uses [Engram v3.2.1](https://github.com/Gentleman-Programming/engram/releases/tag/v3.2.1)
for durable memory. Codync supplies chat capture, automatic extraction, prompt recall and management UI.
ACP still owns the agent conversation and compaction; the built-in **memory MCP server** exposes Engram's
native tools plus Codync's `search_history`. ACP and MCP have different responsibilities.

## Storage and runtime

Each bot has an independent `bots/<id>/engram/engram.db` under `CODYNC_HOME` (normally `~/.codync`).
The host downloads the MIT-licensed release on first use, validates its SHA-256 against
`packaging/engram/release.json`, and installs it with its license in `engram/3.2.1/`.
First use requires network access; subsequent use works offline. A failed download is retried after five
minutes. Memory is best-effort: while Engram can't start, bots answer without it and say so once.
Supported binaries are macOS, Linux and Windows, each on arm64 and x86_64. No global Engram install or HTTP
listener is needed.

The host lazily starts one native MCP stdio process per active bot. Requests serialize within that process;
an idle process stops after five minutes and restarts on demand. A failed request is reported, never silently
replayed: a write may have reached SQLite before its acknowledgement was lost. Inspect memory before retrying.
Engram cloud autosync is disabled. Bot isolation is a separate database and tool routing, not a filesystem
sandbox for an agent that already has shell access.

Engram owns schema migrations and every database write. Codync reads the pinned schema in read-only mode
for structured management, prompt facts and full FTS pagination. Upgrading the pin requires rerunning native
integration tests and checking this schema contract. This is keyword/trigram FTS5 search, not vector similarity.
Short one/two-character queries use escaped substring matching; longer terms use the existing FTS index.
Native MCP search retains Engram's own result limits; the management screen paginates the entire result set.

## Automatic migration

On a bot's first memory access, `memory/profile.md` and `memory/log/*.md` are copied verbatim into an
immutable `engram/markdown-backup.json` import journal. Each dated bullet receives a deterministic sync ID;
its original date, content, profile/log scope and source path/line are imported through the native CLI.
The host verifies every imported ID before writing `.codync-markdown-imported`. Interrupted imports retry
without duplicating records. A bullet whose date can't be read is imported whole, dated by the import, rather
than discarded or blocking the import (the journal is immutable, so editing the Markdown can't fix it).
The original Markdown remains untouched as a backup and is no longer authoritative.

The completion marker is retained by **Forget everything** so old Markdown cannot reappear. Removing the
marker is a deliberate recovery operation, not part of normal startup. To roll back to an older Codync build,
stop the host first and retain both the Engram directory and original Markdown. The original files contain
only pre-migration memory; newer Engram data must be exported separately and is not automatically converted
back into the legacy format.

## Management and automation

Desktop, iOS and terminal expose search, filters (all/about you/pinned/due for review), paginated browsing,
add/edit, scope/category/topic, pin/unpin, mark reviewed, observation history and session timeline, forget,
clear, and JSON export/import. A `personal` or `global` scope remains within this bot's database; it does not
share memories with other bots. The TUI shows operation shortcuts in the memory sheet footer.

The main agent receives native Engram tools, including review, comparison and conflict judgment. Dedicated
conflict adjudication screens are not implemented; those operations are available to the agent through MCP.
The keeper remains responsible for automatically extracting durable facts from completed user conversations,
consolidating a crowded profile, and saving native `session_summary` observations. It runs after five idle
minutes or eight queued exchanges; new-session and compaction events request an early flush. Summaries are
incremental batches of a session. The harness's compacted context is a separate artifact.

Only the user's own messages feed it (and Engram's captured prompts): not bot requests, group rooms or
routine runs. Exchanges remain in the durable Codync queue until processing succeeds; one run takes up to
eight. A failure is logged and retried after ten minutes, doubling up to about two and a half hours; while it
keeps failing, the newest 32 exchanges wait. Explicit memory corrections (edit, forget, clear) invalidate prompt
snapshots and pending old extraction work; in-progress extraction checks a revision before writing. Editing
memory sends a correction notice on the next agent turn. Pinning and importing only re-render the snapshots. It cannot remove text already held in a harness's context; start a new session when needed.
The transcript is retained separately and is still searchable with `search_history`.

**Forget** soft-deletes an observation through Engram. **Forget everything** invokes native hard project
deletion for observations, captured prompts and sessions, discards pending keeper work, and invalidates prompt
snapshots. Original transcripts, explicitly retained migration/import backups, and Engram audit metadata are
not secure-erased. This action clears active memory, not every historical copy on disk.

## Backups and API

Export uses Engram's native direct-backup format, including observations, sessions, prompts, pins, relations
and tombstones supported by that version. Import saves `before-import-<uuid>.json` first and merges using
Engram sync IDs and timestamps. It preserves newer changes; importing an older backup is not guaranteed to
undo a newer deletion. Cross-bot imports remap project/session ownership into the destination's database.
Engram's JSON format does not include the separate observation revision-history table; old version contents
are therefore not restored by a JSON import. For a complete local archive including that history, stop the
host and back up the whole `engram/` directory. Backups larger than 384 KiB use Codync's existing chunked
attachment transport; imports accept up to 100 MiB.
Retained backups and transfer files are local files, so ordinary host backup/retention policies apply.

Authenticated client methods: `memory`, `saveMemory`, `memoryDetail`, `pinMemory`, `reviewMemory`,
`forgetMemory`, `clearMemory`, `exportMemory`, `importMemory`. The local-only MCP bridge uses `memoryTools`
and `memoryCall`; agents receive their real ACP session binding, including independent thread sessions.

## Verification

`packaging/engram/test-runtime.py` downloads and checksum-verifies the same pinned binary used by the host.
Set `CODYNC_TEST_ENGRAM` to its output path, then run:

```sh
cd host
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --test memory_e2e -- --ignored
cargo test native_memory_survives_restart_and_cannot_read_another_bot -- --ignored
```

Host CI repeats the native tests on its platform matrix. The integration fixture always uses disposable data;
it never migrates the developer's real bots.

## Local validation (2.12.0)

- Host: formatting and Clippy; 227 unit tests; 15 host, 10 routine and 4 team integration tests.
- Native Engram: restart/isolation; Markdown migration, Chinese pagination, edits/history/pins, clear including
  captured prompts, cross-bot import; large chunked backups; automatic extraction and native summaries on
  new-session flush. Tests use deterministic ACP fixtures, not a paid model provider.
- Desktop: TypeScript check, 87 tests and production build; browser interaction against a real disposable
  host for add, pin, edit, Chinese search, history, export and clear.
- Terminal: actual PTY interaction for memory browsing, adding and reading history. Footer shortcuts were
  shortened after checking the standard-width sheet.
- iOS: 111 package tests on an iOS 26.5 simulator, including the latest memory views and compatibility floor.
  Physical-device memory UI and Linux/Windows runtime behavior were not exercised locally; the latter run
  in the host CI matrix.

The initial parallel host integration run hit one fixture startup timeout; all 29 host/routine/team
integration cases passed with one test thread. Raising the client host floor exposed an older version in
an unrelated iOS fake-host fixture; it now uses the current floor and the full suite passes.
