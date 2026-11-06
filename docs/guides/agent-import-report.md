# Topic 12: import agents

**Written, not compiled, not run** (at the user's request: gates are left for one pass at the end).

## What changed

- `apps/desktop/src/shared/agent-import.ts` (+ test): the pure importer, ported in design from Rakazo's
  `packages/core/src/agent-import/` (Apache-2.0, `THIRD-PARTY-NOTICES.md`): formats, credential shapes, plan, choice.
  **Not a copy of the 14 files:** they depend on Rakazo's contracts and an archive reader, and a faithful copy could not be
  checked without building; this is one small module with the same file lists and rules.
- `apps/desktop/src/main/agent-import.ts`: the folder reader behind `window.codync.agentImport.pick()` (limits, no symlinks,
  secret files named but not read), registered in `main/index.ts`, `preload/index.ts`, `shared/ipc.ts`.
- `views/settings/AgentImportSection.tsx`: the Import button and the preview with switches, in Settings.
- Host: `host/src/chat/memory/agent_import.rs` and the `importAgentMemory` method; its credential check is a second lock.

## Acceptance (to check on the first run)

"Importing a test folder of each format creates a bot with instructions and memory, and no credential reaches the
database": the test folders are in `shared/agent-import.test.ts` (OpenClaw and Hermes fixtures with a key in `MEMORY.md`);
the Rust check is `chat::memory::agent_import::tests`. Neither has been run.

## Not done

- Reading a `.zip` or tar of an agent (Rakazo reads archives).
- Android, iPhone and terminal (reason in `docs/features/agent-import.md`).
- No screen was seen; the picker was not opened.
