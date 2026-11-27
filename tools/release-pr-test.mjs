// Self-check for the release-pr workflow's routing, with canned agent results: node tools/release-pr-test.mjs
import { readFileSync } from 'node:fs'

const src = readFileSync(new URL('../.claude/workflows/release-pr.js', import.meta.url), 'utf8')
  .replace('export const meta', 'const meta')
const AsyncFunction = (async () => {}).constructor
const body = new AsyncFunction('args', 'agent', 'parallel', 'phase', 'log', 'workflow', src)

// A gate fix's after-change run that reviewed and walked it: one e2e check, nothing failed.
const afterChange = { e2e: { checks: [{ result: 'pass' }] }, review: { confirmed: [] }, fix: {} }

// Each agent label gets the answer of the longest key it starts with; an array answers its calls in turn. The
// 'after-change' answer is that workflow's report (afterChange when not given). It runs as a merge run unless told otherwise; `null` runs it with no args at all, as a plain "pr to main".
const run = async (answers, extra = {}) => {
  const args = extra === null ? {} : { merge: true, ...extra }
  const calls = []
  const seen = {}
  const agent = async (prompt, opts) => {
    calls.push(opts.label)
    const key = Object.keys(answers).filter((k) => opts.label.startsWith(k)).sort((a, b) => b.length - a.length)[0]
    const a = key ? answers[key] : null
    if (typeof a === 'function') return a(prompt, opts)
    if (!Array.isArray(a)) return a
    seen[key] = (seen[key] ?? 0) + 1
    return a[seen[key] - 1] ?? null
  }
  const workflow = async () => (Object.hasOwn(answers, 'after-change') ? answers['after-change'] : afterChange)
  const r = await body(args, agent, async () => [], () => {}, () => {}, workflow)
  return { r, calls }
}

let failures = 0
const check = (ok, what) => { if (!ok) { failures++; console.error('FAIL', what) } }
const gates = (calls) => calls.filter((c) => c.startsWith('gate:')).length

const facts = { version: '2.52.1', latest_tag: 'v2.52.0', needs_bump: false, ships_code: true, nothing_to_release: false, min_app_raised: false, min_app: '2.51.0', cloud_changed: false, crates_to_bump: [], notes_missing: [], open_prs: [] }
const prepared = { status: 'ready', version: '2.52.1', branch: 'release/2.52.1', worktree: '/w', pr: 600, detail: '' }
const sha = (c) => c.repeat(40)
const ready = { status: 'ready', head: sha('a'), findings: [], detail: '' }
const notReady = { status: 'not_ready', head: sha('b'), findings: ['ci: host clippy'], detail: '' }
const fixed = { applied: ['fixed clippy'], rerun: [], refuted: [], needs_maintainer: [], tests: 'ok' }
const merged = { status: 'merged', merge_commit: sha('m'), detail: '' }
const deploys = [{ target: 'relay', changed: true, result: 'deployed' }, { target: 'discord', changed: false, result: 'not_changed' }]
const followed = { status: 'done', deploys, detail: '' }
const base = { inspect: facts, prepare: prepared, merge: merged, 'follow-up': followed }
const capture = (answer) => { const f = (p) => { f.prompt = p; return answer }; return f }

{
  const { r, calls } = await run({ ...base, 'gate:': [ready] })
  check(r.outcome === 'released' && !calls.some((c) => c.startsWith('fix:')), 'a clean gate merges without a fix round')
}
{
  const merge = capture(merged)
  const { r, calls } = await run({ ...base, merge, 'gate:': [notReady, { ...ready, head: sha('c') }], 'fix:': [fixed] })
  check(r.outcome === 'released' && calls.join() === 'inspect,prepare,gate:1,fix:1,gate:2,merge,follow-up', 'a fixed failure merges after the next round: ' + calls)
  check(/gh pr merge 600 /.test(merge.prompt) && merge.prompt.includes('--match-head-commit ' + sha('c')), 'the merge pins the PR and the head the last gate passed')
}
{
  const { r, calls } = await run({ ...base, 'gate:': [notReady, ready], 'fix:': [{ ...fixed, applied: [], rerun: ['run 1'] }] })
  check(r.outcome === 'released' && gates(calls) === 2, 'a rerun alone takes another round')
}
for (const [report, why] of [[null, 'did not finish'], [{ e2e: { checks: [] } }, 'ran no e2e check'], [{ e2e: { checks: [{ result: 'fail' }] } }, 'failed an e2e check']]) {
  const { r, calls } = await run({ ...base, 'gate:': [notReady, ready], 'fix:': [fixed], 'after-change': report })
  check(r.outcome === 'stopped' && r.why.endsWith(why) && gates(calls) === 1 && !calls.includes('merge'), 'gate fixes whose after-change ' + why + ' stop before the next round')
}
{
  const { r, calls } = await run({ ...base, 'gate:': [notReady, ready], 'fix:': [{ ...fixed, applied: [], refuted: [{ item: 'clippy', why: 'no' }] }] })
  check(r.outcome === 'stopped' && gates(calls) === 1 && !calls.includes('merge'), 'a failing gate with nothing fixed or rerun stops at once')
}
{
  const { r, calls } = await run({ ...base, 'gate:': [notReady, notReady, notReady, notReady, ready], 'fix:': [fixed, fixed, fixed, fixed] })
  check(r.outcome === 'stopped' && !calls.includes('merge') && gates(calls) === 4, 'stops after 4 rounds without merging')
}
for (const status of ['timed_out', 'failed']) {
  const { r, calls } = await run({ ...base, 'gate:': [{ ...ready, status }] })
  check(r.outcome === 'stopped' && !calls.includes('merge'), `a gate that ${status} stops`)
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
  check(r.outcome === 'released' && gates(calls) === 1, 'a ready head whose findings are all refuted merges as it is')
  const gate = capture(withFinding)
  const fix = capture(refuted)
  await run({ ...base, 'gate:': gate, 'fix:': fix })
  check(/reviewThreads/.test(gate.prompt) && /isResolved/.test(gate.prompt) && /originalLine/.test(gate.prompt) && !/commit_id is/.test(gate.prompt), 'the gate lists every unresolved Greptile thread, whatever its commit')
  check(/resolveReviewThread/.test(fix.prompt) && /needs_maintainer thread stays open/.test(fix.prompt), 'the fix round resolves the threads it settled and leaves decisions open')
  const decide = await run({ ...base, 'gate:': [withFinding], 'fix:': [{ ...refuted, needs_maintainer: ['rename the setting?'] }] })
  check(decide.r.outcome === 'ask' && !decide.calls.includes('merge'), 'a finding that needs a decision asks instead of merging')
}
{
  const { r, calls } = await run({ ...base, 'gate:': [{ status: 'head_moved', head: 'h2', detail: 'pushed by someone' }] })
  check(r.outcome === 'stopped' && !calls.includes('merge'), 'a moved head stops')
}
{
  const { r } = await run({ ...base, inspect: { ...facts, nothing_to_release: true } })
  check(r.outcome === 'nothing', 'nothing on dev: nothing to do')
}
{
  const { r, calls } = await run({ ...base, merge: { status: 'merge_failed', detail: 'conflict' }, 'gate:': [ready] })
  check(r.outcome === 'stopped' && !calls.includes('follow-up'), 'a failed merge stops before the follow-up')
  const unfinished = await run({ ...base, 'follow-up': null, 'gate:': [ready] })
  check(unfinished.r.outcome === 'merged_incomplete' && /git merge --no-edit origin\/release\/2\.52\.1/.test(unfinished.r.next), 'a merged release whose follow-up did not finish says what is left')
}
{
  const prepare = capture(prepared)
  const followUp = capture(followed)
  const { r } = await run({ ...base, prepare, 'follow-up': followUp, inspect: { ...facts, needs_bump: true }, 'gate:': [ready] })
  check(r.outcome === 'released' && /bump-version\.py patch\|minor/.test(prepare.prompt) && /Auto Tag/.test(followUp.prompt), 'shipped code with the version at the tag bumps and watches the tag')
  const docsOnly = await run({ ...base, prepare, 'follow-up': followUp, inspect: { ...facts, needs_bump: true, ships_code: false }, 'gate:': [ready] })
  check(docsOnly.r.outcome === 'released' && !/bump-version/.test(prepare.prompt) && /no bump/.test(prepare.prompt) && !/cargo build/.test(prepare.prompt) && !/Auto Tag/.test(followUp.prompt), 'no shipped code: no bump, no real agents, no tag to watch')
}
{
  const followUp = capture(followed)
  const cloud = { status: 'ready', main_needs: ['secret FOO (can create)'], detail: '' }
  const { r, calls } = await run({ ...base, 'follow-up': followUp, cloud, inspect: { ...facts, cloud_changed: true }, 'gate:': [ready] })
  check(r.outcome === 'released' && calls.join() === 'inspect,prepare,gate:1,cloud,merge,follow-up', 'cloud/ is checked before the merge: ' + calls)
  check(followUp.prompt.indexOf('deploy:main') < followUp.prompt.indexOf('back into dev') && followUp.prompt.includes('secret FOO'), 'main\'s cloud deploys before the merge back into dev')
  const fetched = followUp.prompt.indexOf('git fetch origin')
  check(fetched >= 0 && fetched < followUp.prompt.indexOf('checkout --detach origin/main') && followUp.prompt.includes('--is-ancestor'), 'main\'s cloud deploys from the merged main, fetched after the merge')
  const cleanupStep = followUp.prompt.slice(followUp.prompt.indexOf('Cleanup:'))
  check(!/pull --rebase --autostash/.test(followUp.prompt) && /pull --ff-only origin dev/.test(cleanupStep), 'cleanup never autostashes the shared checkout')
  const noSha = capture(followed)
  await run({ ...base, merge: { ...merged, merge_commit: undefined }, 'follow-up': noSha, inspect: { ...facts, cloud_changed: true }, cloud, 'gate:': [ready] })
  check(noSha.prompt.includes('--is-ancestor $(gh pr view 600 '), 'a merge without its sha still gets a runnable ancestor check')
  const held = await run({ ...base, cloud: { ...cloud, status: 'needs_maintainer', detail: 'secret BAR' }, inspect: { ...facts, cloud_changed: true }, 'gate:': [ready] })
  check(held.r.outcome === 'ask' && !held.calls.includes('merge'), 'a cloud prerequisite only the maintainer can make asks before merging')
}
{
  const contributor = { number: 700, head: 'fix/google-play-links-main', cross_repository: false }
  const fork = { number: 701, head: 'release/9.9.9', cross_repository: true }
  const oldFlow = { number: 566, head: 'dev', cross_repository: false }
  const prepare = capture(prepared)
  const answers = { ...base, prepare, 'gate:': [ready] }
  const { r } = await run({ ...answers, inspect: { ...facts, open_prs: [contributor, fork, oldFlow] } })
  check(r.outcome === 'released' && r.pr === 600 && /git worktree add/.test(prepare.prompt) && !/#70[01]/.test(prepare.prompt), 'other PRs into main are never reused: a new release branch is cut')
  check(/gh pr close <n>/.test(prepare.prompt) && /#566/.test(prepare.prompt) && r.superseded.join() === '566' && r.other_prs.join() === '700,701', 'a release PR straight from dev is closed as superseded, the others reported')
  const early = await run({ ...answers, inspect: { ...facts, nothing_to_release: true, open_prs: [oldFlow] } })
  check(early.r.outcome === 'nothing' && !early.r.superseded.length && early.r.other_prs.join() === '566', 'nothing is reported closed before prepare ran')
  const own = { number: 650, head: 'release/2.52.1', cross_repository: false }
  const reuse = await run({ ...answers, prepare: capture({ ...prepared, pr: 650 }), inspect: { ...facts, nothing_to_release: true, open_prs: [own] } })
  check(reuse.r.outcome === 'released' && reuse.r.pr === 650, 'our own open release PR is reused')
  const bumped = capture({ ...prepared, version: '2.52.2', branch: 'release/2.52.2', pr: 651 })
  const moved = await run({ ...answers, prepare: bumped, inspect: { ...facts, needs_bump: true, open_prs: [own] } })
  check(moved.r.outcome === 'released' && moved.r.pr === 651 && /gh pr create/.test(bumped.prompt) && /close #650/.test(bumped.prompt) && /--delete release\/2\.52\.1/.test(bumped.prompt), 'a reused release PR that needs a bump moves to a new PR and closes the old one')
  const kept = await run({ ...answers, prepare: capture({ ...prepared, version: '2.52.2', branch: 'release/2.52.2', pr: 650 }), inspect: { ...facts, needs_bump: true, open_prs: [own] } })
  check(kept.r.outcome === 'stopped', 'a bumped release still on the old PR stops')
  const other = await run({ ...answers, inspect: { ...facts, open_prs: [own] } })
  check(other.r.outcome === 'stopped' && !other.calls.some((c) => c.startsWith('gate:')), 'a prepare result naming another PR than the open release PR stops')
}
{
  const { r, calls } = await run({ ...base, 'gate:': [{ ...ready, head: 'h1' }] })
  check(r.outcome === 'stopped' && !calls.includes('merge'), 'a gate without a full head sha never merges')
  for (const bad of [{ branch: 'dev' }, { branch: 'release/2.52.2' }]) {
    const none = await run({ ...base, prepare: { ...prepared, ...bad }, 'gate:': [ready] })
    check(none.r.outcome === 'stopped' && !none.calls.some((c) => c.startsWith('gate:')), 'a prepare result whose branch is not release/<its version> stops: ' + bad.branch)
  }
}

{
  const plainCall = await run({ ...base, 'gate:': [ready] }, null)
  check(plainCall.r.outcome === 'ready' && !['cloud', 'merge', 'follow-up'].some((c) => plainCall.calls.includes(c)), 'a plain "pr to main" with no args stops at the green gate: ' + plainCall.calls)
  const { r, calls } = await run({ ...base, 'gate:': [notReady, ready], 'fix:': [fixed] }, { merge: false })
  check(r.outcome === 'ready' && r.pr === 600 && !calls.includes('merge') && !calls.includes('follow-up'), 'without merge: true it stops once the gate passes: ' + calls)
  const cloudRun = await run({ ...base, inspect: { ...facts, cloud_changed: true }, 'gate:': [ready] }, { merge: false })
  check(cloudRun.r.outcome === 'ready' && !cloudRun.calls.includes('cloud'), 'without merge: true the cloud check waits for the merge run')
  const own = { number: 650, head: 'release/2.52.1', cross_repository: false }
  const prepare = capture({ ...prepared, pr: 650 })
  const again = await run({ ...base, prepare, 'gate:': [ready], inspect: { ...facts, open_prs: [own] } })
  check(again.r.outcome === 'released' && /Skip this step when #650/.test(prepare.prompt), 'the merge run reuses the gated PR without rerunning the real agents')
  const plain = capture({ ...prepared, pr: 650 })
  const first = await run({ ...base, prepare: plain, 'gate:': [ready], inspect: { ...facts, open_prs: [own] } }, { merge: false })
  check(first.r.outcome === 'ready' && !/Skip this step/.test(plain.prompt), 'a run without merge on an open release PR still runs the real agents')
  const raised = { ...base, inspect: { ...facts, min_app_raised: true, min_app: '2.51.0 -> 2.52.0' }, 'gate:': [ready] }
  const approved = await run(raised, { merge: false, allowMinAppRaise: true })
  check(approved.r.outcome === 'ready' && /merge: true/.test(approved.r.next) && /allowMinAppRaise: true/.test(approved.r.next), 'a ready MIN_APP release names both args for the merge run: ' + approved.r.next)
  const asked = await run(raised)
  check(asked.r.outcome === 'ask' && /allowMinAppRaise: true/.test(asked.r.next) && /merge: true/.test(asked.r.next), 'a merge run asking about MIN_APP keeps merge in its rerun: ' + asked.r.next)
  const both = await run(raised, { allowMinAppRaise: true })
  check(both.r.outcome === 'released', 'both args release a MIN_APP raise')
  const held = await run({ ...base, cloud: { status: 'needs_maintainer', main_needs: [], detail: 'secret BAR' }, inspect: { ...facts, cloud_changed: true }, 'gate:': [ready] })
  check(/rerun with \{merge: true\}/.test(held.r.next), 'a merge run held at the cloud check reruns as a merge run')
}

{
  const followUp = capture(followed)
  const { r } = await run({ ...base, merge: { ...merged, merge_commit: sha('d') }, 'follow-up': followUp, inspect: { ...facts, cloud_changed: true }, cloud: { status: 'ready', main_needs: [], detail: '' }, 'gate:': [ready] })
  const p = followUp.prompt
  const workers = p.indexOf('Deploy each Worker')
  const w = p.slice(workers, p.indexOf('Bring the release branch back'))
  const cloud = p.slice(p.indexOf('cloud/ changed'), workers)
  check(r.outcome === 'released' && workers > p.indexOf('deploy:main') && workers < p.indexOf('back into dev'), 'the Workers deploy after the cloud and before the merge back into dev')
  check(/relay\/, discord\//.test(w) && w.includes('git diff --quiet ' + sha('d') + '^1 ' + sha('d') + ' -- <dir>') && w.includes('`npm ci`') && w.includes('`npm run deploy`'), 'each Worker deploys only when the merge changed it')
  check(/exiting 0 means not_changed/.test(w) && /1 means changed/.test(w) && /any other exit .* failed/.test(w), 'a git diff that errors fails the Worker instead of skipping it')
  check(/note the live version/.test(cloud) && /wrangler rollback <the noted version>/.test(cloud), 'a broken cloud deploy rolls back to the version noted before it')
  check(w.includes('push.codync.dev/health') && w.includes('discord.codync.dev/') && /wrangler rollback/.test(w) && /stops here/.test(w), 'each Worker checks its real path and rolls back a broken version')
  for (const [rows, why] of [[undefined, 'no rows'], [[deploys[0]], 'a Worker missing'], [[deploys[0], { target: 'discord', changed: true, result: 'rolled_back' }], 'a rolled-back Worker'], [[deploys[0], { target: 'discord', changed: true, result: 'failed' }], 'a failed Worker']]) {
    const bad = await run({ ...base, 'follow-up': { ...followed, deploys: rows }, 'gate:': [ready] })
    check(bad.r.outcome === 'merged_incomplete' && /Deploy each Worker/.test(bad.r.next) && /no deployed result for: [a-z, ]*discord\./.test(bad.r.next), 'a follow-up with ' + why + ' is not released and names the Worker')
  }
}

if (failures) process.exit(1)
console.log('release-pr: all checks passed')
