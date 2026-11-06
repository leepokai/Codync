export const meta = {
  name: 'after-change',
  description: 'Review, fix and clean up a finished code change in one pass, then end-to-end test it (docs/guides/clean-code.md)',
  whenToUse: 'After every code change once its tests pass, before commit. Run it from the change\'s worktree. args: {change: what changed and which flows to walk, base?: git ref (default origin/dev), e2e?: "minimal" | "full"}',
  phases: [
    { title: 'Review', detail: 'correctness, completeness and refactor reviewers at once, then one verifier that refutes the bug findings' },
    { title: 'Fix', detail: 'one agent: confirmed findings, refactors worth doing and a cleanup pass, one rebuild' },
    { title: 'E2E', detail: 'the changed flows on the real apps, once; a fixed failure is walked again alone' },
  ],
}

const input = typeof args === 'string' ? { change: args } : (args ?? {})
const change = input.change ?? 'the uncommitted and unpushed change in this worktree'
const base = input.base ?? 'origin/dev'
const e2e = input.e2e === 'full' ? 'full' : 'minimal'
const scope = `The change: ${change}
Its diff: \`git diff ${base}\` (committed and uncommitted) plus the new files \`git status --short\` lists, in the current directory (the change's git worktree). Touch nothing outside that worktree and no other agent's work.`

const FINDING = {
  type: 'object',
  properties: {
    file: { type: 'string' },
    line: { type: 'integer' },
    severity: { type: 'string', enum: ['high', 'medium', 'low'] },
    summary: { type: 'string' },
    failure_scenario: { type: 'string', description: 'concrete inputs or state and the wrong result; for a refactor, what it costs to leave as is' },
    fix: { type: 'string' },
  },
  required: ['file', 'severity', 'summary', 'failure_scenario', 'fix'],
}
const FINDINGS = { type: 'object', properties: { findings: { type: 'array', items: FINDING } }, required: ['findings'] }

const VERDICTS = {
  type: 'object',
  properties: {
    confirmed: { type: 'array', items: FINDING },
    refuted: { type: 'array', items: { type: 'object', properties: { summary: { type: 'string' }, why: { type: 'string' } }, required: ['summary', 'why'] } },
  },
  required: ['confirmed', 'refuted'],
}

const FIXED = {
  type: 'object',
  properties: {
    applied: { type: 'array', items: { type: 'string' } },
    skipped: { type: 'array', items: { type: 'object', properties: { item: { type: 'string' }, why: { type: 'string' } }, required: ['item', 'why'] } },
    tests: { type: 'string', description: 'which builds and tests ran and their result' },
  },
  required: ['applied', 'skipped', 'tests'],
}

const CHECKS = {
  type: 'object',
  properties: {
    checks: {
      type: 'array',
      items: {
        type: 'object',
        properties: {
          platform: { type: 'string' },
          flow: { type: 'string' },
          result: { type: 'string', enum: ['pass', 'fail', 'preexisting', 'not_run'], description: 'preexisting: broken the same way without this change, so not its failure' },
          detail: { type: 'string', description: 'what was seen; for fail or preexisting the symptom, for not_run why' },
        },
        required: ['platform', 'flow', 'result', 'detail'],
      },
    },
  },
  required: ['checks'],
}

// Review and verification always run on Opus or above, whatever model started the session.
const REVIEW_MODEL = 'opus'

const REVIEWERS = [
  { name: 'correctness', text: 'Lens: correctness: bugs, regressions, broken edge cases, error handling, data loss, security, and compatibility with older clients and hosts (docs/reference/compatibility.md). Report only real defects with a concrete failure scenario; style nits are not findings.' },
  { name: 'completeness', text: 'Lens: completeness: every client the change should reach (iPhone, Android, desktop, terminal UI, per CLAUDE.md cross-platform rules), docs made stale by it (the user manual in web/content/docs/ too, for anything users see or do differently: docs/guides/user-docs.md), missing or weak tests, and the version bump and What\'s New rules. Report only real gaps; style nits are not findings.' },
  { name: 'refactor', text: 'Lens: refactor (docs/guides/clean-code.md "After every change"): responsibility splits, files past ~500 lines, long functions, layer leaks, missed reuse, language best practices (Swift 6, Rust rust-skills, strict TypeScript, Compose). Only suggestions worth their diff.' },
]

phase('Review')
const reviews = await parallel(REVIEWERS.map((r) => () =>
  agent(`Review a code change, read-only (edit nothing). ${r.text}
${scope}
Most important first. Return an empty list when there is nothing.`,
    { label: `review:${r.name}`, phase: 'Review', schema: FINDINGS, model: REVIEW_MODEL })))
const [bugs, gaps, refactors] = reviews.map((r) => r?.findings ?? [])
const found = [...bugs, ...gaps]
let confirmed = []
let refuted = []
if (found.length) {
  const verdict = await agent(`Verify review findings on a code change, read-only. For each finding, read the code and try to refute it: is the failure scenario real in this code? Default to refuted when you can't reproduce the reasoning. Merge duplicates.
${scope}
Findings:
${JSON.stringify(found, null, 2)}`, { label: 'verify', phase: 'Review', schema: VERDICTS, model: REVIEW_MODEL })
  confirmed = verdict?.confirmed ?? found
  refuted = verdict?.refuted ?? []
}
log(`Review: ${found.length} found, ${confirmed.length} confirmed, ${refactors.length} refactors suggested`)

phase('Fix')
// One editor for findings, refactors and cleanup: one pass over the files and one rebuild, not three.
const fixed = await agent(`Edit the change in one pass, then rerun once the builds and tests of the parts you touched (CLAUDE.md lists each part's checks).
${scope}
1. Fix each confirmed finding: the smallest correct fix at its root cause.
${JSON.stringify(confirmed, null, 2)}
2. Apply the refactor suggestions worth their diff; skip the rest with one line why each.
${JSON.stringify(refactors, null, 2)}
3. Clean up the change's own lines (docs/guides/clean-code.md "After every change"): dead code, duplication, unclear names, comments that restate the code or no longer match it, unused imports, places an existing helper should be reused. Keep behavior identical and match the surrounding code.`,
  { label: 'fix', phase: 'Fix', schema: FIXED })

const walk = (flows, round) => agent(`End-to-end test a code change on the real apps, ${e2e} e2e as CLAUDE.md "End-to-end test every change" defines it (minimal: only this computer's platform; full: also Lima Linux and the UTM Windows VM, skipping and reporting a VM that isn't installed).
${scope}
${flows}
Walk each flow once, as a person would, until it clearly passes or fails, and note what you saw; don't explore beyond the listed flows. Keep it quick: short messages to bots, no sleeps or long-running tasks to stretch a turn, and when \`host/\` is unchanged start \`npm run dev\` with \`CODYNC_HOST_BIN\` pointing at an already built host instead of building one (docs/guides/dev-app.md). A failure the change didn't cause (the same without it) is \`preexisting\`, not \`fail\`. Stop the old build before installing the new one. In this worktree \`npm run dev\` in apps/desktop runs an isolated sandbox (docs/guides/dev-app.md#worktree-sandboxes); never stop or replace the shared Codync Dev, the production Codync app or another session's sandbox, and when done leave this worktree's \`npm run dev\` running (it is stopped after the merge) but stop anything else you started. A change with nothing to walk (CI, tools, docs) reports one not_run check saying so.`,
  { label: `e2e:${round}`, phase: 'E2E', schema: CHECKS })

phase('E2E')
const first = await walk('Walk the flows the change touches.', 1)
let checks = first?.checks ?? []
const failures = checks.filter((c) => c.result === 'fail')
let e2eFix = null
if (failures.length) {
  e2eFix = await agent(`Fix these e2e failures in the change, the smallest correct fix at the root cause, then rerun the builds and tests the fix touches.
${scope}
${JSON.stringify(failures, null, 2)}`, { label: 'fix:e2e', phase: 'E2E', schema: FIXED })
  // Walk again only what was fixed: an unchanged change fails the same way.
  if (e2eFix?.applied.length) {
    const again = await walk(`This is the second walk, after fixes for these failures of the first. Walk only these flows:\n${JSON.stringify(failures.map((f) => `${f.platform}: ${f.flow}`), null, 2)}`, 2)
    checks = [...checks.filter((c) => c.result !== 'fail'), ...(again?.checks ?? failures)]
  }
}

return {
  review: { found: found.length, confirmed, refuted },
  fix: { refactors, ...fixed },
  e2e: { checks, fix: e2eFix },
  next: 'Unless an e2e check failed or a skipped fix matters: commit in this worktree, rebase on origin/dev, fast-forward dev, push origin dev, pull dev in the main checkout. Only once that merge is pushed (never before, and not at all if the merge or push fails): stop this worktree\'s `npm run dev` (its Vite, Electron app and sandbox host) and check none of it remains, then leave the worktree and, from the main checkout, delete it: `git worktree remove --force <worktree>`, `git branch -D <branch>`, then `npm run dev -- --prune` in apps/desktop.',
}
