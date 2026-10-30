# Clean code

Rules every change follows, in every part of the repository (host, iOS, desktop, cloud, relay). Language-specific rules stay in [CLAUDE.md](../../CLAUDE.md) (Swift 6, Rust lints, TypeScript); where to put a file is in [file structure](../architecture/file-structure.md).

## Before writing code

- **Reuse first.** Search for an existing helper, type, component or pattern before adding one (`LockExt`, `Motion`, `Controls.swift`, `components/`, store methods). A second copy of logic that already exists is a bug waiting to diverge.
- **Build only what is asked.** No speculative options, config for values that never change, or scaffolding "for later". No protocol/trait/interface with one implementation unless it is the test seam.
- **Fix the root cause** in the shared function every caller goes through, not a guard at the one call site the bug report names.

## Modules

- **One responsibility per file and per type.** If describing a file needs "and", it is two files. Name it after that responsibility, using the shared terms from file structure.
- **Don't grow a large file.** New behavior in a file past ~500 lines goes into a new sibling module (Rust submodule, Swift extension file, React component/hook), not appended to it. When a change already touches a large file, move the part you are changing out first, as its own `refactor` commit.
- **Layers stay in their lane.** Views render state and send intents; stores/models own state and logic; clients/transports own I/O. No networking, persistence or parsing in a SwiftUI view or React component. In the host, keep protocol parsing, decisions and side effects (db, process, network) apart so the decision part is a pure, unit-testable function. Across the apps, product logic lives in the host, not in any client: [host owns the logic](../architecture/overview.md#host-owns-the-logic).
- **Prefer a ready-made component over hand-rolling; don't reinvent the wheel.** Order: existing code here, platform framework, an already-installed dependency, then a mature external library. Mature means long and well run: years of releases, wide use, still maintained (an official SDK, a de-facto standard crate or package). When one covers the need, use it instead of writing our own, above all for protocols and transports, parsers, crypto, retries and time handling, where a homegrown copy carries its bugs alone. Hand-write only glue a few lines long, or what no mature library covers. Skip one only when it breaks the platform look or a design rule here (shadcn/ui, Zustand and react-resizable-panels are not wanted on desktop).
- **Small public surface.** Default to `private`/`fileprivate`, `pub(crate)` or no export; widen only when another module needs it.
- **Dependencies point one way.** `CodyncKit` never imports `CodyncUI`; renderer code reaches the host only through `window.codync`; shared code never knows about a specific screen.

## Functions and types

- **Short functions at one level of abstraction.** A function either orchestrates (calls named steps) or does one step, not both. Past ~40 lines or 3 levels of nesting, extract named helpers.
- **Early return** over nested `if`/`else`; `guard` in Swift, `?`/`let … else` in Rust.
- **Make illegal states unrepresentable.** Enums with associated data over flags, optional pairs and string states; newtypes/IDs over bare `String`/`Int` where mixing them up is possible.
- **No magic values.** Name constants once, next to the code that owns them; reuse theme tokens instead of literal colors, spacing and durations.
- **Pure where possible.** Pass data in and return results; keep mutation and global state at the edges.

## Readability

- **Names say intent**, not type or mechanics (`pendingApprovals`, not `list2` or `dataArray`). Booleans read as questions (`isConnected`, `hasUnread`).
- **Comments explain why**, never restate what the code does. A comment that explains *what* means the code needs a better name or an extracted function.
- **Delete dead code.** No commented-out code, unused parameters, stale feature flags or `TODO` without a reason. Git keeps the history.
- **Errors are never swallowed.** Handle, propagate with context, or log with `tracing`/`Logger`; an empty `catch` or `let _ =` on a `Result` needs a comment saying why it is safe.

## Changing existing code

- Leave the code you touch a bit cleaner than you found it, but keep cleanups inside your change's area and in separate `refactor` commits from behavior changes.
- Prefer deleting and simplifying over adding. The smallest diff that fixes the problem in the right place wins.
- Every changed piece of logic gets a test that would fail without the change; test behavior, not private details.

## After every change: review, e2e, cleanup

Once a code change works (its tests pass) and before commit, it goes through three stages, in this order:

1. **Review**: three read-only reviewers at once: correctness (bugs, regressions, error handling, security, compatibility with older clients and hosts), completeness (every client the change should reach, stale docs and the [user manual](user-docs.md) page, missing tests, version and What's New rules) and refactor (responsibility splits, files past ~500 lines, long functions, layer leaks, missed reuse, language best practices: Swift 6, Rust `rust-skills`, strict TypeScript, Compose). A verifier then tries to refute each correctness and completeness finding against the code. In Claude Code the reviewers and the verifier always run on Opus (`REVIEW_MODEL` in the workflow), whichever model started the session; the fix and e2e agents inherit the session's model.
2. **Fix and cleanup**, one editor in one pass: the surviving findings at their root cause, the refactors worth their diff, and a cleanup of the change's own lines (dead code, duplication, unclear names, comments that restate the code, unused imports, places an existing helper should be reused); then the affected builds and tests run once.
3. **End-to-end test** of the final code: the changed flows on the real apps, minimal or full as CLAUDE.md defines it, each walked once with short bot turns. A failure the change didn't cause is reported as preexisting. A real failure is fixed, and only the fixed flows are walked again.

In Claude Code, the `after-change` workflow (`.claude/workflows/after-change.js`) runs all three: start it from the change's worktree with `{"change": "<what changed and which flows to walk>", "e2e": "minimal"}` (`"full"` for platform code, packaging or the desktop main process; `"base"` if the change isn't against `origin/dev`). It returns the confirmed and refuted findings, what the fix pass changed or skipped (refactors included), and the e2e checks. The worktree's `npm run dev` keeps running through it.

In Codex, use the repository's [after-change skill](../../.agents/skills/after-change/SKILL.md). It uses Codex subagents for the three reviews and verification, then one editor and one E2E tester; without subagents, the coordinator performs the same stages. It accepts the same `change`, `base` and `e2e` inputs in ordinary language or as an object. For example:

```text
$after-change change="Fix desktop reconnect; walk disconnect and reconnect" base=origin/dev e2e=minimal
```

The skill lives in `.agents/skills/after-change`, the repository skill location supported by [Codex](https://learn.chatgpt.com/docs/build-skills#where-codex-loads-local-skills). If it is missing from the current session's skill list, ask Codex to read that `SKILL.md` directly, or restart Codex to refresh discovery. This is a skill-driven workflow, not a Claude JavaScript runner or an automatic hook. Keep the two implementations aligned when changing this procedure.

Once it's green, the change is committed, merged into `dev` and pushed. Only after that merge is pushed are the worktree's `npm run dev` (its Electron app and sandbox host) stopped and the worktree deleted (a failed merge or push keeps both): stop it, leave the worktree, then from the main checkout `git worktree remove --force <path>`, `git branch -D <branch>` and `npm run dev -- --prune` in `apps/desktop` (deletes its sandbox). Nothing started from the worktree keeps running.

Agents touch only the change's own hunks, never another agent's uncommitted work. Keep refactors in `refactor` commits apart from the behavior change. The final report lists the review findings (fixed, or refuted and why), each e2e check that ran or couldn't and why, what the cleanup changed, and each refactor applied or skipped with one line why. Docs- or config-only changes skip this; a tool without subagents does the stages itself.
