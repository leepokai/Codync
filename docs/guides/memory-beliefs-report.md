# Topic 13: memory as beliefs

**Written, not compiled, not run** (the user asked for the gates to wait for one pass at the end).

## What changed

- `host/crates/memory/src/files/belief.rs` (new, with tests): `Kind`, `Confidence`, `Belief`, `read`, `stamp` for the header.
- `host/src/chat/memory/beliefs.rs` (new, with `beliefs/tests.rs`): list with filters, confirm / forget / restore, the bot's
  `confirm_memory` / `correct_memory` / `forget_memory`, the "I learned" notice with Undo (`memoryUndo` in the entry),
  `undoMemoryChange`, and the 24-hour limit on contradiction questions. Wired in `memory/mod.rs`, `tools.rs` (definitions,
  dispatch and the bot's instructions) and `api/bots.rs`.
- Desktop: `shared/beliefs.ts` (+ test), `MemoryStore.loadBeliefs/beliefAction`, `BeliefsSection.tsx`, the **What I know** tab
  in `MemoryView.tsx`, Undo in `NoticeRow`, `BotStore.undoMemoryChange`, `EntryData.memoryUndo`.
- Android: `core/Beliefs.kt` (+ `BeliefsTest`), `BeliefsView.kt`, the tab in `MemoryCard.kt`, `LearnedNotice` in `ChatRows.kt`.
- Terminal: `tui/app/beliefs.rs` (+ tests), `tui/view/beliefs.rs`, the "What this bot knows about you…" command.

## Decisions

- The header is per **note**, in the frontmatter the notes already have (the `description:` line is required); `MEMORY.md`
  facts have no per-fact metadata.
- Forgetting is the existing deletion version, so "a new version, not a hard delete" is the memory history's own rule.
- Only `correct_memory` and `forget_memory` announce a change: confirming changes no behavior.

## Not tested

Nothing was compiled or run. The Rust tests use a temporary data folder; the bot-tool test accepts the write rules refusing
its stand-in turn message (it then checks that nothing half-happened). No screen was seen.
