---
name: after-change
description: Review, fix, clean up, and end-to-end test a finished Codync code change of our own before commit (issues and PRs from other people follow docs/guides/issue-pr-review.md). Use after affected tests pass or when explicitly asked to run the after-change workflow.
---

# After change

Run Codync's review → fix → E2E workflow with Codex's available agent and computer-use tools. This is an instruction-driven skill; it does not execute the Claude workflow JavaScript or require Claude Code.

## Scope and inputs

Resolve the repository root from the current working directory. Read its `AGENTS.md` and [clean-code guide](../../../docs/guides/clean-code.md#after-every-change-review-e2e-cleanup), plus instructions for the affected directories. Resolve all repository paths against that root, including when invoked from `host/` or another subdirectory.

Accept ordinary language or an object with:

- `change`: what changed and which flows to walk; default to this task's committed and uncommitted work.
- `base`: comparison ref, default `origin/dev`.
- `e2e`: `minimal` or `full`; default to minimal, but use full for platform code, packaging, desktop main-process changes, or an explicit full request.

Record the absolute worktree path, base commit, owned files/hunks, relevant tests already run, and changed user flows. Inspect `git diff <base>` and `git status --short`, including new files. The diff is candidate scope, not permission to change other agents' work. If the base does not exist, report it and resolve the intended base before review. Do not reset or stash work to obtain a clean diff.

For docs/config-only changes with no runtime behavior, validate the affected artifacts and report E2E as `not_run` with the reason; do not launch apps or the full review pipeline. Executable tools and behavior-changing configuration still need applicable checks. Finish prerequisite tests before reviewing code, or explicitly report why they cannot run.

## Review

Use three read-only subagents in parallel when delegation is available; otherwise perform the three reviews yourself and disclose the fallback. Respect the session's concurrency limit and reuse finished agents for later phases.

Give every reviewer the same absolute worktree, base commit, owned scope, change description, and relevant repository instructions. They must not edit, commit, start apps, or spawn further agents.

| Reviewer | Focus |
| --- | --- |
| Correctness | Concrete bugs, regressions, error handling, security, and compatibility with older hosts/clients; consult `docs/reference/compatibility.md`. |
| Completeness | All affected clients, stale documentation (including the user manual page, `docs/guides/user-docs.md`), missing behavioral tests, release versions and iOS What's New requirements. |
| Refactor | Responsibility splits, files over ~500 lines, long functions, layer leaks, missed reuse, and applicable language conventions. Only suggestions worth their diff. |

Each finding includes file, line when available, severity (`high`, `medium`, `low`), summary, concrete failure scenario (or cost of leaving a refactor), and suggested fix. Empty results are valid. Style preferences alone are not defects.

After all reviews finish, have a read-only verifier try to refute each correctness/completeness finding against the code. Deduplicate findings; return `confirmed` and `refuted`, with evidence or reasons. Unsupported claims are refuted rather than silently promoted to bugs; unavailable evidence must be disclosed. Keep refactor suggestions separate. If a reviewer or verifier fails, retry or perform that work yourself; missing output is not an empty successful review.

## Fix and cleanup

Assign one editor ownership of the scoped files/hunks, or do the editing yourself. Tell any editor it is not alone in the codebase: preserve other agents' edits and adapt to them. Reviewers remain read-only, and subagents never commit or merge.

In one pass, fix confirmed findings at their root cause, apply worthwhile refactors, and clean up the change's own dead code, duplication, unclear names/comments and unused imports. Keep unrelated work out. Record applied changes and skipped suggestions with reasons. Follow the repository's release/version rules when shipped code changes; do not bump for this skill or documentation alone.

Run the affected builds/tests after the edits. Reuse still-valid results; rerun when new edits or failures warrant it. Do not proceed to app E2E with an unresolved build/test failure. Preserve enough separation to commit refactors apart from behavior changes as required by the repository.

## E2E

Give one tester (or yourself) the final change, exact flows and E2E mode. Read the relevant parts of `docs/guides/development.md`, `docs/guides/dev-app.md`, and, for Windows, `docs/guides/windows-testing.md` before starting apps.

- Stop only this task's old app/host processes before rebuilding or installing. Never stop the shared Codync Dev, production app, or another session's sandbox. Track what you start.
- Prefer `npm run dev` in `apps/desktop` in the change's isolated worktree. When testing from the shared checkout, use its documented sandbox mode. Restart for main-process/preload changes. Package only when the flow needs capabilities unavailable unpackaged.
- When `host/` is unchanged, reuse a compatible built host through `CODYNC_HOST_BIN` as the development guide describes. When it changed, build the correct host for this worktree. Apple build output stays in this checkout's `build/dd`.
- Walk the changed flows on the actual apps as the [test stack](../../../docs/guides/test-stack.md) says: the desktop with the whole Playwright suite (`npm run e2e` against `npm run dev -- --cdp`); the phones with one Maestro run under the Maestro lock, only the `smoke` flows plus the tags of the areas the change touches (`--include-tags`, `--shard-all`), never the whole Maestro suite for now; Computer Use only for the native steps that guide lists. Add or update only the spec or flow each changed flow needs and touch no other test. Edit an existing test only for a reason that matches the behavior the change says it changes, and list each such edit with its reason; without such a reason the failing test found a regression, so report it as a fail instead of changing the test. iPhone checks use the worktree's own iOS Simulator (`Codync <sandbox name>`: `npm run dev` prints its udid, or the command that creates it when there is none yet; `npm run dev -- --prune` deletes it); Android checks use the shared `codync_worktree` emulator, the only one that may run (never a second AVD, never `codync_api36`, the maintainer's), one test at a time under its lock, with `pm clear` and the notification grant first ([test devices](../../../docs/guides/dev-app.md#worktree-sandboxes)). Builds and unit tests do not substitute for UI checks. No changed client UI here means no UI parity work.
- Host tests run one at a time: never start a test binary several times in parallel, or several copies of the suite, not even to hunt a flake (repeat it one run after another, a few times at most).
- Minimal E2E covers this computer's platform. Full E2E also covers installed Lima Linux and UTM Windows VMs; report absent VMs rather than creating them. Validate affected clients as required by repository instructions, and report unavailable tools/devices explicitly.
- Walk each flow once, using short bot messages. Record `platform`, `flow`, `result` (`pass`, `fail`, `preexisting`, `not_run`), and observed evidence or reason. Use `preexisting` only with evidence it fails without this change; otherwise retain the uncertainty as a failure. Never disturb another worktree to prove a baseline.

Fix change-caused E2E failures, run the checks touched by the fix, and walk only the failed flows again. Keep retries bounded to one E2E fix/retest round; if still failing, report unresolved failures and keep the work available for follow-up instead of calling it green.

## Report and handoff

Report:

- Review: confirmed findings and their disposition; refuted findings and why; incomplete review work.
- Fix: fixes/cleanup/refactors applied, suggestions skipped and why, builds/tests and results.
- E2E: each check's platform, flow, result and evidence; checks not run and why.
- Next: readiness to commit, any unresolved failures or material skipped fixes, and any task-owned processes still running.

Do not label missing validation as passed. Follow the existing task's commit/merge/push authorization and the repository's handoff rules; the skill itself does not authorize publishing unrelated work. Only the coordinator performs git handoff. Passing on the worktree is enough, even when `dev` has moved on: a rebase needs only its conflicts resolved and what they touched still building.

Keep the change's development app running through review and until its merge is successfully pushed. On merge/push failure, preserve the worktree and its app. After successful push, if the change touched `cloud/`, deploy it to dev right away from the worktree, now the clean tree at `origin/dev` (never the shared main checkout): only when `git rev-parse HEAD` equals `git rev-parse origin/dev` and `git status --porcelain -- cloud` prints nothing, else stop and report; new migrations, bindings and secrets first, then `npm run deploy:dev` in `cloud/` ([releasing cloud changes](../../../docs/guides/environments-and-deployment.md#releasing-cloud-changes)). Then stop the task-owned app, Vite, sandbox host and local cloud, verify they exited, leave the worktree, and follow the clean-code guide's worktree/sandbox cleanup. Confirm the worktree contains no unmerged or other agents' work before removal; never remove the shared checkout. Stop other temporary processes you started when no longer needed.
