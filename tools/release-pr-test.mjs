// Self-check for the release-pr workflow's routing, with canned agent results: node tools/release-pr-test.mjs
import { readFileSync } from 'node:fs'

const src = readFileSync(new URL('../.claude/workflows/release-pr.js', import.meta.url), 'utf8')
  .replace('export const meta', 'const meta')
const AsyncFunction = (async () => {}).constructor
const body = new AsyncFunction('args', 'agent', 'parallel', 'phase', 'log', 'workflow', src)

// Each agent label gets the answer of the longest key it starts with; an array answers its calls in turn.
const run = async (answers, args = {}) => {
  const calls = []
  const seen = {}
  const agent = async (prompt, opts) => {
    calls.push(opts.label)
    const key = Object.keys(answers).filter((k) => opts.label.startsWith(k)).sort((a, b) => b.length - a.length)[0]
    const a = key ? answers[key] : null
    if (!Array.isArray(a)) return a
    seen[key] = (seen[key] ?? 0) + 1
    return a[seen[key] - 1] ?? null
  }
  const r = await body(args, agent, async () => [], () => {}, () => {}, async () => null)
  return { r, calls }
}

let failures = 0
const check = (ok, what) => { if (!ok) { failures++; console.error('FAIL', what) } }

const facts = { version: '2.52.1', latest_tag: 'v2.52.0', needs_bump: false, nothing_to_release: false, min_app_raised: false, cloud_changed: false, crates_to_bump: [], notes_missing: [], open_pr: 0 }
const prepared = { status: 'ready', version: '2.52.1', branch: 'release/2.52.1', worktree: '/w', pr: 600, detail: '' }
const ready = { status: 'ready', head: 'h1', findings: [], detail: '' }
const notReady = { status: 'not_ready', head: 'h1', findings: ['ci: host clippy'], detail: '' }
const fixed = { applied: ['fixed clippy'], rerun: [], refuted: [], needs_maintainer: [], tests: 'ok' }
const released = { status: 'released', detail: '' }
const base = { inspect: facts, prepare: prepared, 'release': released }

{
  const { r, calls } = await run({ ...base, 'gate:': [ready] })
  check(r.outcome === 'released' && !calls.some((c) => c.startsWith('fix:')), 'a clean gate merges without a fix round')
}
{
  const { r, calls } = await run({ ...base, 'gate:': [notReady, ready], 'fix:': [fixed] })
  check(r.outcome === 'released' && calls.join() === 'inspect,prepare,gate:1,fix:1,gate:2,release', 'a fixed failure merges after the next round: ' + calls)
}
{
  const { r, calls } = await run({ ...base, 'gate:': [notReady, notReady, notReady, notReady, ready], 'fix:': [fixed, fixed, fixed, fixed] })
  check(r.outcome === 'stopped' && !calls.includes('release') && calls.filter((c) => c.startsWith('gate:')).length === 4, 'stops after 4 rounds without merging')
}
{
  const { r, calls } = await run({ ...base, inspect: { ...facts, min_app_raised: true, min_app: '2.51.0 -> 2.52.0' } })
  check(r.outcome === 'ask' && calls.join() === 'inspect', 'a MIN_APP raise asks before anything')
  const again = await run({ ...base, inspect: { ...facts, min_app_raised: true }, 'gate:': [ready] }, { allowMinAppRaise: true })
  check(again.r.outcome === 'released', 'allowMinAppRaise releases it')
}
{
  const { r, calls } = await run({ ...base, prepare: { status: 'real_agents_failed', detail: 'codex failed' } })
  check(r.outcome === 'stopped' && !calls.some((c) => c.startsWith('gate:')), 'a failed real-agent check stops before the PR gate')
}
{
  const withFinding = { ...ready, findings: ['greptile: P2 naming'] }
  const refuted = { ...fixed, applied: [], refuted: [{ item: 'P2 naming', why: 'not a defect' }] }
  const { r, calls } = await run({ ...base, 'gate:': [withFinding], 'fix:': [refuted] })
  check(r.outcome === 'released' && calls.filter((c) => c.startsWith('gate:')).length === 1, 'a ready head whose findings are all refuted merges as it is')
  const decide = await run({ ...base, 'gate:': [withFinding], 'fix:': [{ ...refuted, needs_maintainer: ['rename the setting?'] }] })
  check(decide.r.outcome === 'ask' && !decide.calls.includes('release'), 'a finding that needs a decision asks instead of merging')
}
{
  const { r, calls } = await run({ ...base, 'gate:': [{ status: 'head_moved', head: 'h2', detail: 'pushed by someone' }] })
  check(r.outcome === 'stopped' && !calls.includes('release'), 'a moved head stops')
}
{
  const { r } = await run({ ...base, inspect: { ...facts, nothing_to_release: true } })
  check(r.outcome === 'nothing', 'nothing on dev: nothing to do')
}

if (failures) process.exit(1)
console.log('release-pr: all checks passed')
