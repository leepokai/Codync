// Self-check for the review-issue-pr workflow's routing, with canned agent results: node tools/review-issue-pr-test.mjs
import { readFileSync } from 'node:fs'

const src = readFileSync(new URL('../.claude/workflows/review-issue-pr.js', import.meta.url), 'utf8')
  .replace('export const meta', 'const meta')
const AsyncFunction = (async () => {}).constructor
const body = new AsyncFunction('args', 'agent', 'parallel', 'phase', 'log', 'workflow', src)

// Runs the workflow; each agent label gets the answer of the longest key it starts with (a function sees the prompt).
const run = async (args, answers) => {
  const calls = []
  const prompts = {}
  const agent = async (prompt, opts) => {
    calls.push(opts.label)
    prompts[opts.label] = prompt
    const key = Object.keys(answers).filter((k) => opts.label.startsWith(k)).sort((a, b) => b.length - a.length)[0]
    const a = key ? answers[key] : null
    return typeof a === 'function' ? a(prompt, opts) : a
  }
  const parallel = (thunks) => Promise.all(thunks.map((t) => t().catch(() => null)))
  const workflow = async (name, a) => { calls.push(`workflow:${name}:${a.dir}`); prompts[`workflow:${name}`] = a.change; return answers.afterChange }
  const r = await body(args, agent, parallel, () => {}, () => {}, workflow)
  return { results: r.results, outcome: r.results.map((x) => x.outcome).join(','), calls, prompts, ran: (p) => calls.some((c) => c.startsWith(p)) }
}

let failures = 0
const check = (ok, what) => { if (!ok) { failures++; console.error('FAIL', what) } }

const head = 'a'.repeat(40)
const facts = { title: 't', author: 'someone', external: true, head_sha: head, head_ref: 'fix/x', base: 'dev', cross_repository: true, maintainer_can_modify: true }
const clean = { verdict: 'clean', findings: [], checked: 'x', facts }
const bug = { kind: 'bug_fix', approach: 'sound', deviations: [], excessive: false, contribution: 'real', reasons: '', author_owes: [], summary: 's' }
const feature = { ...bug, kind: 'feature', excessive: true }
const real = { problem: 'real', evidence: 'e', test: 't', nothing_to_walk: false, pr_fixes_it: 'yes', greptile_ok: false, blocked: 'none', worktree: '/w', branch: 'pr-1' }
const ac = { review: { confirmed: [] }, e2e: { checks: [{ result: 'pass' }] }, fix: { applied: [], skipped: [] }, next: 'handoff' }
const drafts = { drafts: [{ on: '#1', as: 'comment', text: 'x' }], cleanup: '' }
const merged = { status: 'merged', commit: 'm', detail: '' }
const ours = { applied: ['bounded the retries'], skipped: [], tests: 'ok' }
const pr = { 'security:': clean, 'judge:': bug, 'confirm:': real, 'fix:': ours, afterChange: ac, 'ship:': merged, 'drafts:': drafts }

// A routine bug-fix PR: Greptile low at first, its findings fixed, merged on round 2 with the merge pinned to a sha.
let ships = 0
let x = await run('pr 1', { ...pr, 'fix:gate:': { applied: ['f'], skipped: [], tests: 'ok' },
  'ship:': () => (++ships === 1 ? { status: 'not_ready', commit: 'c1', findings: ['f'], detail: '' } : merged) })
check(x.outcome === 'merged' && x.ran('fix:gate:'), `bug-fix PR merges after a gate fix: ${x.outcome}`)
check(x.prompts['ship:pr#1:2'].includes('--match-head-commit') && x.prompts['ship:pr#1:2'].includes('c1'), 'round 2 pins the merge and accepts our push')
check(!x.ran('fix:pr#1'), 'a sound PR gets no fix pass before after-change')

// Anything beyond a routine bug fix asks before any worktree or app run.
x = await run('pr 2', { ...pr, 'judge:': feature })
check(x.outcome === 'ask' && !x.ran('confirm:') && !x.ran('workflow:'), `feature asks first: ${x.outcome}`)
x = await run('pr 11', { ...pr, 'judge:': { ...bug, contribution: 'farming' } })
check(x.outcome === 'ask' && !x.ran('confirm:'), `farming asks: ${x.outcome}`)
x = await run({ items: 'pr 2', approved: [2] }, { ...pr, 'judge:': feature })
check(x.outcome === 'changes', `approved feature below Greptile 4 asks for changes: ${x.outcome}`)

// Security: a stop, a lens that died, or lenses that read different heads stop the item before anything runs.
x = await run('pr 3', { ...pr, 'security:injection': { ...clean, verdict: 'stop' } })
check(x.outcome === 'stop' && x.calls.length === 2, `injection stops: ${x.calls}`)
x = await run('pr 4', { ...pr, 'security:injection': null })
check(x.outcome === 'stop' && !x.ran('judge:'), 'missing lens stops')
x = await run('pr 12', { ...pr, 'security:injection': { ...clean, facts: { ...facts, head_sha: 'b'.repeat(40) } } })
check(x.outcome === 'stop' && !x.ran('judge:'), 'lenses on different heads stop')
x = await run('pr 13', { ...pr, 'security:': { ...clean, facts: { ...facts, head_sha: undefined } } })
check(x.outcome === 'stop', 'a PR without a head sha stops')

// Only PRs into dev: the maintainer's release PR stops; an external PR into main goes on (and is retargeted).
x = await run('pr 14', { ...pr, 'security:': { ...clean, facts: { ...facts, base: 'main', external: false } } })
check(x.outcome === 'stop' && !x.ran('judge:'), `maintainer PR into main stops: ${x.outcome}`)
x = await run('pr 15', { ...pr, 'security:': { ...clean, facts: { ...facts, base: 'main' } } })
check(x.outcome === 'merged' && x.prompts['confirm:pr#15'].includes('--base dev'), `external PR into main is retargeted: ${x.outcome}`)

// Confirm blockers: only a dev conflict is the author's; our leftover work and a moved head keep the worktree.
x = await run('pr 7', { ...pr, 'confirm:': { ...real, blocked: 'head_moved' } })
check(x.outcome === 'unfinished' && !x.ran('drafts:'), `head moved: ${x.outcome}`)
x = await run('pr 16', { ...pr, 'confirm:': { ...real, blocked: 'leftover_work', detail: 'uncommitted fixes' } })
check(x.outcome === 'unfinished' && !x.ran('drafts:') && x.results[0].why === 'uncommitted fixes', `leftover work is kept: ${x.outcome}`)
x = await run('pr 17', { ...pr, 'confirm:': { ...real, blocked: 'conflict' } })
check(x.outcome === 'changes' && x.prompts['drafts:pr#17'].includes('git worktree remove'), `dev conflict asks for changes: ${x.outcome}`)

// A same-repo branch reports maintainerCanModify false but takes our pushes.
const sameRepo = { ...clean, facts: { ...facts, cross_repository: false, maintainer_can_modify: false } }
x = await run('pr 18', { ...pr, 'security:': sameRepo, 'judge:': { ...bug, approach: 'needs_changes' } })
check(x.outcome === 'merged' && x.ran('fix:pr#18'), `same-repo PR needing edits merges after our fix pass: ${x.outcome}`)
x = await run('pr 19', { ...pr, 'security:': { ...clean, facts: { ...facts, maintainer_can_modify: false } }, 'judge:': { ...bug, approach: 'needs_changes' } })
check(x.outcome === 'changes', `fork we can't push to asks for changes: ${x.outcome}`)
x = await run('pr 31', { ...pr, 'confirm:': { ...real, pr_fixes_it: 'partly' } })
check(x.outcome === 'merged' && x.calls.indexOf('fix:pr#31') > -1 && x.calls.indexOf('fix:pr#31') < x.calls.findIndex((c) => c.startsWith('workflow:')), `a partial fix is finished by us before after-change: ${x.outcome}`)
x = await run({ items: 'pr 32', approved: [32] }, { ...pr, 'judge:': { ...feature, approach: 'needs_changes' }, 'confirm:': { ...real, greptile_ok: true } })
check(x.outcome === 'changes' && !x.ran('fix:pr'), `approved feature needing edits goes back to its author: ${x.outcome}`)
const deviation = [{ rule: 'docs kept current', where: 'docs/x.md', why: 'stale' }]
x = await run('pr 34', { ...pr, 'judge:': { ...bug, deviations: deviation } })
check(x.outcome === 'merged' && x.ran('fix:pr#34') && x.prompts['fix:pr#34'].includes('stale'), `a sound bug fix with rule breaks gets our fix pass: ${x.outcome}`)
x = await run({ items: 'pr 35', approved: [35] }, { ...pr, 'judge:': { ...feature, deviations: deviation }, 'confirm:': { ...real, greptile_ok: true } })
check(x.outcome === 'changes' && !x.ran('fix:pr'), `approved feature with rule breaks goes back to its author: ${x.outcome}`)

// A routine bug fix's security concerns are defects we fix on its branch; anything bigger sends them to its author.
const concerns = { ...clean, verdict: 'concerns', findings: [{ where: 'a.rs:1', severity: 'medium', summary: 'unbounded retries', evidence: 'loop {}' }] }
x = await run('pr 27', { ...pr, 'security:': concerns })
check(x.outcome === 'merged' && x.prompts['fix:pr#27'].includes('unbounded retries'), `bug fix with security concerns is finished by us: ${x.outcome}`)
check(x.calls.indexOf('fix:pr#27') < x.calls.findIndex((c) => c.startsWith('workflow:')), 'its defects are fixed before after-change')
check(x.prompts['drafts:pr#27'].includes('bounded the retries'), 'the thank-you lists our fixes')
check(x.prompts['workflow:after-change'].includes('unbounded retries'), 'after-change checks each security finding is gone')
x = await run('pr 33', { ...pr, 'security:': concerns, 'fix:': { ...ours, applied: [], skipped: [{ item: 'unbounded retries', why: 'does not hold' }] } })
check(x.outcome === 'unfinished' && !x.ran('workflow:') && x.results[0].why.includes('unbounded retries'), `a skipped security finding stops for the maintainer: ${x.outcome}`)
x = await run('pr 28', { ...pr, 'security:': { ...concerns, facts: { ...facts, maintainer_can_modify: false } } })
check(x.outcome === 'changes' && !x.ran('fix:pr'), `security concerns on a fork we can't push to ask for changes: ${x.outcome}`)
x = await run({ items: 'pr 29', approved: [29] }, { ...pr, 'security:': concerns, 'judge:': feature, 'confirm:': { ...real, greptile_ok: true } })
check(x.outcome === 'changes' && !x.ran('fix:pr'), `approved feature with security concerns asks for changes: ${x.outcome}`)
x = await run('pr 30', { ...pr, 'security:': concerns, 'fix:': null })
check(x.outcome === 'unfinished' && !x.ran('workflow:'), `PR fix pass died → unfinished: ${x.outcome}`)

// after-change must show the fix working; anything less leaves the item unfinished and unshipped.
for (const [what, report, extra] of [
  ['e2e fail', { ...ac, e2e: { checks: [{ result: 'fail' }] } }, {}],
  ['no e2e checks', { ...ac, e2e: { checks: [] } }, {}],
  ['every check not_run', { ...ac, e2e: { checks: [{ result: 'not_run' }] } }, {}],
  ['after-change died', null, {}],
  ['fix pass died with confirmed findings', { ...ac, review: { confirmed: [{}] }, fix: { refactors: [] } }, {}],
]) {
  x = await run('pr 8', { ...pr, afterChange: report, ...extra })
  check(x.outcome === 'unfinished' && !x.ran('ship:'), `${what} → unfinished: ${x.outcome}`)
}
x = await run('pr 20', { ...pr, 'confirm:': { ...real, nothing_to_walk: true }, afterChange: { ...ac, e2e: { checks: [{ result: 'not_run' }] } } })
check(x.outcome === 'merged', `a tools-only fix with nothing to walk merges: ${x.outcome}`)
x = await run('pr 21', { ...pr, afterChange: { ...ac, review: { confirmed: [{ summary: 'bug' }] }, fix: { applied: [], skipped: [{ item: 'bug', why: 'later' }] } } })
check(x.prompts['ship:pr#21:1'].includes('status failed'), 'ship checks skipped confirmed findings')

// Ship: a moved head keeps the worktree, unpushable branches ask for changes.
x = await run('pr 22', { ...pr, 'ship:': { status: 'head_moved', detail: 'new commit' } })
check(x.outcome === 'unfinished' && !x.ran('drafts:'), `head moved while shipping: ${x.outcome}`)
x = await run('pr 23', { ...pr, 'ship:': { status: 'cannot_push', detail: '' } })
check(x.outcome === 'changes', `cannot push: ${x.outcome}`)

// Issues: landed, or a reply when not reproduced; a fix that didn't finish never reaches after-change.
const issue = { ...pr, 'confirm:': { ...real, pr_fixes_it: 'not_applicable', branch: 'fix/issue-5-x' }, 'fix:': { applied: ['a'], skipped: [], tests: 'ok' }, 'land:': { status: 'landed', commit: 'c', detail: '' } }
x = await run('issue 5', issue)
check(x.outcome === 'landed', `issue lands: ${x.outcome}`)
x = await run('issue 6', { ...issue, 'confirm:': { ...issue['confirm:'], problem: 'not_reproduced' } })
check(x.outcome === 'reply' && x.prompts['drafts:issue#6'].includes('fix/issue-5-x'), `not reproduced → reply: ${x.outcome}`)
x = await run('issue 24', { ...issue, 'fix:': null })
check(x.outcome === 'unfinished' && !x.ran('workflow:'), `fix died → unfinished: ${x.outcome}`)

// A judge that died leaves its item unfinished, and the items before it keep their results.
x = await run({ items: 'pr 25, pr 26', approved: [26] }, { ...pr, 'judge:pr#26': null })
check(x.outcome === 'merged,unfinished', `dead judge: ${x.outcome}`)

// Every security check runs before any item goes further; an item needs its kind.
x = await run('pr 9, issue 10', { ...pr, 'judge:': feature })
check(x.calls.slice(0, 3).every((c) => c.startsWith('security:')), `security first: ${x.calls}`)
let threw = false
try { await run('#12', {}) } catch { threw = true }
check(threw, 'kindless item rejected')

if (failures) process.exit(1)
console.log('ok')
