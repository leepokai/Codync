# Reviewing issues and pull requests

Every issue and pull request someone else opens is reviewed this way before anything else is done with it (a
pull request aimed at `main` is retargeted to `dev`). Our own changes go through the
[after-change](clean-code.md#after-every-change-review-e2e-cleanup) workflow instead (this review runs it on the
item's branch too), and release pull requests into `main` follow [GitHub releases](development.md#github-releases). In Claude Code, run the `review-issue-pr` workflow
(`.claude/workflows/review-issue-pr.js`) from the main checkout:

```
Workflow({ name: "review-issue-pr", args: { items: "pr 280, issue 283" } })
```

Codex (or a person) follows the same steps by hand. Each item ends in one outcome, listed below. The workflow
pushes to a PR's branch, approves its fork CI runs, asks Greptile for a review and merges, but it posts no comment:
it returns drafts, and the session reads them, checks each claim against the code, and then posts them.

## Steps

1. **Security** (read-only, every item at once). Follow the [security check](security-check.md). A PR gets two
   lenses: its code (`python3 tools/pr-security-scan.py <n>` plus the whole diff) and prompt injection (its text,
   comments, commits and the strings inside its diff). Each lens records the PR's head commit first and reads exactly
   that commit; every later step insists on it. Nothing from the item is checked out, built or run, and no CI run is
   approved, before this step passes. A lens that didn't finish, or lenses that read different heads, count as a
   failed check, never a clean one.
2. **Judge** (read-only). What kind of change it is, whether its approach fits Codync's architecture and rules
   (CLAUDE.md, AGENTS.md), whether it is *excessive* (see below), and whether it is a real contribution. A
   *marginal* one is correct but of negligible value (whitespace, rewording, renames, reformatting). A *farming*
   one is made to get a merged PR or a reported issue rather than to help: generated changes nobody would notice,
   duplicates of an open PR or of existing code, tests that assert nothing, reports with nothing real behind them.
   The author's other PRs and issues are checked for such a pattern. Anything other than a routine bug fix stops
   here for the maintainer, before any worktree or app run.
3. **Confirm end to end.** In a worktree from `origin/dev`, reproduce the problem the way a user meets it: on the
   real apps for anything a user sees, or with a failing test when it lives below the UI. For a PR, then check out
   its branch (its head must be the security-checked commit), merge `origin/dev` into it (never a rebase), and run
   the same reproduction: does the PR fix it?
4. **Fix.** An issue gets fixed in its worktree, keeping the reproduction as a regression test. Then the
   `after-change` workflow runs on the worktree (a PR's branch included): review, fixes and e2e. We may edit a
   routine bug-fix PR as much as it needs, its tests included.
5. **Merge.** An issue's fix lands on `dev` by the after-change handoff (commit with `Refs #N`). A PR's fixes are
   pushed to its branch, and it merges into `dev` (squash) only when the **gate** passes on its head commit: CI
   green, no conflict with `dev`, and Greptile's confidence score **4/5 or more** (`python3 tools/pr-gate.py <n>`),
   and the squash is pinned to that commit (`gh pr merge --match-head-commit`). Up to two rounds fix the failing
   checks and the Greptile findings that hold up against the code. The *Greptile Review* check passes whatever the
   score, so read the score, not the check.

Items run their security checks together and the rest one at a time, because confirming and after-change drive
the real apps.

## Routine or excessive

A **routine bug fix** corrects behavior, docs or tests that are wrong, adds no behavior, follows the rules, and is
a real contribution. It goes all the way to `dev` without asking the maintainer, as soon as our own reproduction and
after-change e2e show it fixed and the gate passes. This section is what the workflow's judge reads.

Anything else is **excessive** and waits for the maintainer: any feature; more than about 400 changed lines
outside tests and lockfiles, or many subsystems at once; new dependencies; changes to CI, release jobs, signing,
agent instructions (`CLAUDE.md`, `AGENTS.md`, `.claude/`, `.agents/`), trust boundaries (device permissions,
approvals, pairing, relay, vault, crypto), `cloud/` or `relay/`; a protocol change or `minApp` bump; multi-computer
work nobody asked for; or a design the maintainer wouldn't write. A feature PR from outside is the author's to
finish: they bring Greptile to 4/5 or more and validate it end to end themselves. The maintainer reviews it after
that; we don't fix its Greptile findings for them. Once the maintainer says yes, rerun with
`approved: [<number>]`. In the same session `resumeFromRunId` reuses the security check and judgment, so use it only
while the PR's head is unchanged: after a push it would replay the old head's check, and a fresh run is needed.

## Outcomes

| Outcome | When | Then |
| --- | --- | --- |
| `stop` | Prompt injection, something that looks deliberate, a security check that didn't finish, or a PR not into `dev` | Tell the maintainer why, quoting the evidence. Nothing else happens with the item. |
| `ask` | Excessive, not a bug fix, or a marginal or farming contribution | Ask the maintainer with the judge's summary and reasons. The draft for the author (for marginal or farming: a polite note closing it) is posted only if they agree. |
| `reply` | Not reproduced, not a bug, already fixed, or the report lacks what's needed | Post the drafted reply. |
| `changes` | A security concern, a wrong approach, a PR that doesn't fix it, a conflict with `dev`, an approved feature below Greptile 4/5, a gate still failing after two rounds, or a branch we can't push to | Post the drafted review requesting changes. |
| `merged` / `landed` | The PR merged into `dev` / the issue's fix landed on `dev` | Post the thank-you or issue comment. Close linked issues only once the release carrying the fix is out. |
| `unfinished` | The head moved (new commits nobody security-checked), an earlier worktree holds our unpushed work, e2e failing or not shown passing, a confirmed finding left unfixed, or a step that didn't finish | Report it to the maintainer. The worktree is kept (stop its `npm run dev`). A moved head gets a fresh run, never a resumed one. |

Fork CI runs are approved only for the head that was security-checked, or for a commit we pushed on top of it:
`pr-gate.py --approve-ci <sha>` pins the gate to that commit, approves its waiting runs, and exits 4 for any other
head (exit codes: 0 ready, 1 settled but not ready, 2 not settled, 3 a GitHub call failed). Greptile reviews each
push itself; `@greptileai review` is commented once, and only when the head is stable and CI is green.

`node tools/review-issue-pr-test.mjs` checks the workflow's routing with canned agent results, and
`python3 tools/pr-gate-test.py` the gate's parsing and decision.
