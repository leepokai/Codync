# Harness memory

Status: the switch that keeps a harness's own memory on **beside** Codync's shipped in 2.19.0 (below). Giving a bot
its harness's memory **instead of** Codync's is planned, not released. The owner (2026-10-19) wants users to choose
freely between the two later; a host-side draft exists only on the owner's machine (branch `feat/harness-memory`,
not pushed), so treat the design below as the spec. Current behavior:
[context and memory](../features/context-and-memory.md).

## Shipped: keep the harness's memory on (2.19.0)

- Every bot runs with its harness's own memory off where Codync can turn it off, project folders included (until
  2.18 a project bot kept Claude Code's). Project knowledge goes in the bot's own notes, and into `CLAUDE.md` or
  `AGENTS.md` only when the user asks.
- The memory screen's "This bot" section has a switch, "Claude Code's own memory", with an info button: it keeps
  notes apart from Codync's; with both on the bot may remember things twice and the two can disagree; forgetting
  something in Codync doesn't reach it; where to read it. The terminal asks the same on `m` before turning it on.
- Offered where Codync controls it (`own_memory::switchable`): Claude Code, Copilot, CodeBuddy and WorkBuddy in any folder,
  Qwen Code only in the bot's own workspace (its switch is a file in the folder, and a project folder's files are
  the user's). Gemini CLI, Kilo Code, Kiro and Hermes Agent have no switch Codync can use.
- Kept on, the harness sets its memory itself (Claude Code's auto-dream too); Claude Code's `MEMORY.md` shows
  read-only in a "Claude Code" section. Changing it restarts the agent, which resumes its session where it can.
- Wire, additive: bots carry `harnessMemory` (bool, `null` without a switch); `updateBot` takes it, and an edit
  to another harness turns it off (`own_memory::settle`).

## Planned: the harness's memory instead of Codync's

Some users will prefer the harness's memory alone, for example to share one memory between a bot and their own
Claude Code sessions in the terminal. The choice adds an off switch for Codync's memory beside today's:

| | Codync's memory (default) | The harness's memory only |
|---|---|---|
| Memory in the instructions and `<memory_update>` notices | Yes | None, including "About you" |
| Memory tools | All | `search_history` only (the chat history is Codync's) |
| Memory folders in `additionalDirectories` | Yes | No |
| Dreaming and the daily time review | Yes | Skipped; its bookmark moves on |
| Memory screen | As today | The switches, a line on what the bot uses, and the harness's notes read-only where Codync can read them |

- **Offered only where the harness has a memory**, or the bot is left with none. A bot moved to a harness without
  one goes back to Codync's.
- **Turning Codync's memory off starts a fresh conversation**: the instructions and tools change, and resuming
  would mix the two memories.
- **Nothing is deleted.** Codync's memory files (`MEMORY.md` and the notes) stay; turning it back on brings them
  back. Dreaming skips the time spent without it.
- **What the detail text says the user gives up**: "About you" shared across bots, editing from the iPhone (only
  Claude Code's notes are readable; Copilot's are in the cloud), the "forgotten stays forgotten" rule and the write
  checks (passwords, keys, planted instructions), and memory that follows the bot to another agent.

The draft (`chat::memory::on`, the `history`-only memory server, the skipped dreaming, the restart with a fresh
session) was written as a one-or-the-other `MemoryOwner`; rebase it as a Codync on/off beside `harnessMemory`, then
do the apps (iPhone, desktop, terminal; Android once its memory screen is ported), the docs, a minor release and a
Mac e2e.

## Open questions

- Gemini CLI's memory is one global `~/.gemini/GEMINI.md` shared by every Gemini session, not per bot. Since 2.25.0
  the memory screen says so for Gemini CLI and Hermes Agent ("Shared" with an info button,
  [context and memory](../features/context-and-memory.md)).
- Codex's memories are off unless the user turns them on in their own config. Offer the choice when that config
  has them on.
