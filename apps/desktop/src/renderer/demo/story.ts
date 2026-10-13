import type { Bot, EntryData, Usage } from '@shared/models'

// The website demo's story: four bots on a small `pace` project (an isoWeek / weeklyTotals bug),
// the "Ship room" group, a thread and a pending approval. Same story as web/public/screens.

export const COMPUTER_ID = 'demo-mac'
export const HOST_NAME = 'MacBook Pro'
const CWD = '/Users/demo/code/pace'

export type SeedBot = Pick<Bot, 'id' | 'name' | 'description' | 'avatarColor' | 'avatarShape' | 'backend' | 'model'> &
  Partial<Pick<Bot, 'kind' | 'members' | 'permission' | 'status' | 'activity' | 'unread'>>

export const bots: SeedBot[] = [
  { id: 'pacer', name: 'Pacer', description: 'Owns the pace math', avatarColor: 'orange', avatarShape: 'squircle', backend: 'claude', model: 'opus', permission: 'ask', status: 'needsInput', activity: 'Needs approval: Edit src/pace.js', unread: 1 },
  { id: 'scout', name: 'Scout', description: 'Finds things in the repo and builds them', avatarColor: 'green', avatarShape: 'cloud', backend: 'codex', model: null, unread: 1 },
  { id: 'reviewer', name: 'Reviewer', description: 'Reviews every change before it ships', avatarColor: 'violet', avatarShape: 'blob', backend: 'claude', model: 'sonnet', unread: 1 },
  { id: 'relay', name: 'Relay', description: 'Writes the release notes', avatarColor: 'blue', avatarShape: 'tablet', backend: 'codex', model: null, unread: 1 },
  { id: 'ship', kind: 'group', members: ['pacer', 'scout', 'reviewer'], name: 'Ship room', description: 'Getting weeklyTotals out', avatarColor: 'gray', avatarShape: 'blob', backend: '', model: null },
]

export const cwd = CWD

export interface SeedEntry {
  id: string
  botId: string
  kind: string
  /** Minutes ago. */
  at: number
  data: EntryData
  threadId?: string
}

const user = (id: string, botId: string, at: number, text: string, threadId?: string): SeedEntry => ({ id, botId, kind: 'user', at, data: { text, status: 'sent' }, threadId })
const reply = (id: string, botId: string, author: string, at: number, text: string, threadId?: string): SeedEntry => ({ id, botId, kind: 'agent', at, data: { text, final: true, author }, threadId })
const thought = (id: string, botId: string, at: number, text: string): SeedEntry => ({ id, botId, kind: 'thought', at, data: { text, author: botId } })
const tool = (id: string, botId: string, at: number, title: string, toolKind: string, output = ''): SeedEntry => ({ id, botId, kind: 'tool', at, data: { title, toolKind, status: 'completed', output, author: botId } })

const weeklyPatch = [
  '@@ -18,3 +18,15 @@ export function isoWeek(date) {',
  ' ',
  '+export function weeklyTotals(runs) {',
  '+  const weeks = {}',
  '+  for (const run of runs) {',
  '+    const key = isoWeek(run.date)',
  '+    const week = (weeks[key] ??= { distance: 0, time: 0 })',
  '+    week.distance += run.distance',
  '+    week.time += run.time',
  '+  }',
  '+  return weeks',
  '+}',
].join('\n')

const isoWeekPatch = [
  '@@ -4,9 +4,10 @@ export function isoWeek(date) {',
  '   const d = new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), date.getUTCDate()))',
  '-  const year = d.getUTCFullYear()',
  '   d.setUTCDate(d.getUTCDate() + 4 - (d.getUTCDay() || 7))',
  '+  // The ISO year is the year of the Thursday of this week.',
  '+  const year = d.getUTCFullYear()',
  '   const start = new Date(Date.UTC(year, 0, 1))',
  '   const week = Math.ceil(((d - start) / 86400000 + 1) / 7)',
  '   return `${year}-W${String(week).padStart(2, "0")}`',
].join('\n')

export const entries: SeedEntry[] = [
  // Scout builds weeklyTotals.
  user('s1', 'scout', 58, 'Add a weeklyTotals(runs) helper that sums distance and time per ISO week, with a test.'),
  thought('s2', 'scout', 58, 'isoWeek already lives in src/pace.js, so weeklyTotals can key on it and stay small.'),
  tool('s3', 'scout', 57, 'Read src/pace.js', 'read'),
  { id: 's4', botId: 'scout', kind: 'tool', at: 57, data: { title: 'Edit src/pace.js', toolKind: 'edit', status: 'completed', author: 'scout', diffs: [{ path: 'src/pace.js', added: 10, removed: 0, isNew: false, startLine: 18, patch: weeklyPatch }] } },
  { id: 's5', botId: 'scout', kind: 'tool', at: 56, data: { title: 'npm test', toolKind: 'execute', command: 'npm test', status: 'completed', output: '✓ isoWeek: mid year\n✓ isoWeek: first week\n✓ weeklyTotals: sums one week\n✓ weeklyTotals: week that crosses into 2027\n\n4 passing (12ms)', author: 'scout' } },
  reply('s6', 'scout', 'scout', 55, 'I added `weeklyTotals(runs)` to `src/pace.js`. It takes runs shaped like `{ date, distance, time }`, groups them by ISO week in UTC (keys like `"2026-W53"`) and sums distance and time. The new test covers the week that crosses into 2027. All 4 tests pass and nothing is committed yet.'),

  // Reviewer looks it over.
  user('r1', 'reviewer', 50, 'Review weeklyTotals before we ship it.'),
  tool('r2', 'reviewer', 49, 'Read src/pace.js', 'read'),
  tool('r3', 'reviewer', 49, 'Read test/pace.test.js', 'read'),
  reply('r4', 'reviewer', 'reviewer', 48, 'I found no blockers. The ISO week math is right, including the New Year edge.\n\nTwo small things:\n- A bad date becomes a `"NaN-WNaN"` bucket instead of an error.\n- Weeks are grouped in UTC, so a late evening run west of UTC can land in the next week.'),

  // Relay writes it up; a thread on its reply.
  user('l1', 'relay', 41, 'Summarize what changed in pace today for the team channel.'),
  reply('l2', 'relay', 'relay', 40, 'Added `weeklyTotals` to group run distance and time by ISO week, with a test for the 2026 to 2027 rollover. Reviewer found no blockers. An invalid-date guard is being added in Ship room.'),
  user('l3', 'relay', 38, 'Make it one line for the changelog.', 'l2'),
  reply('l4', 'relay', 'relay', 38, 'Pace now shows your weekly distance and time totals.', 'l2'),

  // Ship room.
  user('g1', 'ship', 22, 'Is weeklyTotals ready to ship? One short sentence each.'),
  reply('g2', 'ship', 'reviewer', 21, 'Almost: the week math is right and all 4 tests pass, but a bad date gets bucketed as `"NaN-WNaN"` instead of raising an error. @Scout, can you add an invalid-date guard?'),
  reply('g3', 'ship', 'scout', 19, 'Added the guard: `weeklyTotals` now throws a `RangeError` on an invalid date, and all 5 tests pass. Weeks are still grouped in UTC, so say if they should follow the runner\'s local time instead.'),
  user('g4', 'ship', 17, 'UTC is right, keep it.', 'g3'),
  reply('g5', 'ship', 'scout', 17, 'Got it, UTC stays. I left a comment above `weeklyTotals` so nobody flips it later.', 'g3'),
  reply('g6', 'ship', 'pacer', 15, 'Ship it once my `isoWeek` New Year fix lands. Dec 30, 2024 still comes out as 2024-W01 instead of 2025-W01.'),

  // Pacer waits for approval.
  user('p1', 'pacer', 9, 'Fix the isoWeek New Year bug you mentioned.'),
  thought('p2', 'pacer', 9, 'The year is read before moving to the Thursday of the week, so late December dates get the wrong ISO year.'),
  tool('p3', 'pacer', 8, 'Read src/pace.js', 'read'),
  { id: 'p4', botId: 'pacer', kind: 'agent', at: 7, data: { text: 'Found it: the year comes from the date itself, not from the Thursday of its week.', final: false, author: 'pacer' } },
  {
    id: 'p5', botId: 'pacer', kind: 'permission', at: 6,
    data: {
      title: 'Edit src/pace.js', toolKind: 'edit', cwd: CWD, status: 'pending', selected: undefined, author: 'pacer',
      diffs: [{ path: 'src/pace.js', added: 2, removed: 1, isNew: false, startLine: 4, patch: isoWeekPatch }],
      options: [
        { optionId: 'allow', name: 'Allow', kind: 'allow_once' },
        { optionId: 'always', name: 'Always allow', kind: 'allow_always' },
        { optionId: 'deny', name: 'Deny', kind: 'reject_once' },
      ],
    },
  },
]

/** What a bot says back to a new message: rotated so repeated sends read differently. */
export const replies: Record<string, string[]> = {
  pacer: [
    'Sure. I\'ll keep it inside `src/pace.js` and rerun `npm test` when it\'s done.',
    'Makes sense. The week math is the risky part, so I\'ll add a test for it first.',
    'Good call. Nothing else imports that function, so it\'s a one-file change.',
  ],
  scout: [
    'On it. I\'ll grep the repo first so we know what else touches it.',
    'Done a quick pass: only `src/pace.js` and its test use it, so the change stays small.',
    'Sure. I\'ll write the test first, then make it pass.',
  ],
  reviewer: [
    'I\'ll look at the diff once it lands and flag anything that isn\'t a nit.',
    'Looks fine to me. Just keep the test for the New Year week, that\'s where it broke before.',
    'One thing to watch: dates without a time zone are read as UTC. Worth a comment.',
  ],
  relay: [
    'Here\'s a draft: "Pace now groups your runs by week, with totals for distance and time."',
    'Noted. I\'ll fold that into the release notes for the next version.',
    'Short version for the channel: weekly totals are in, a New Year week fix is next.',
  ],
}

export const waitingReply = 'I\'m still waiting on your approval for the `isoWeek` edit. Once you allow it, I\'ll keep going.'
export const approvedReply = 'Fixed: `isoWeek` now takes the year from the Thursday of the week, so Dec 30, 2024 is 2025-W01. I added a test for it and all 6 tests pass.'
export const deniedReply = 'OK, I left `src/pace.js` alone. Want me to explain the fix so you can make it yourself?'

export function usage(now: number): Usage {
  const hours = (h: number) => now + h * 3_600_000
  return {
    providers: [
      { id: 'claude', name: 'Claude', source: 'statusline', updatedAt: now, windows: [
        { id: 'five_hour', label: '5-hour', percent: 38, resetsAt: hours(2.4) },
        { id: 'seven_day', label: 'Weekly', percent: 61, resetsAt: hours(70) },
      ] },
      { id: 'codex', name: 'Codex', source: 'sessions', updatedAt: now, windows: [
        { id: 'primary', label: '5-hour', percent: 22, resetsAt: hours(3.1) },
        { id: 'secondary', label: 'Weekly', percent: 47, resetsAt: hours(95) },
      ] },
    ],
  }
}
