import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  ACTION_CLASSES,
  BOT_AVATAR_LOOKS,
  WORKER_ROLE_TEMPLATES,
  collectDeliverables,
  collectPendingDecisions,
  defaultProactiveSettings,
  draftOf,
  isChat,
  isReplyable,
  normalizeBot,
  polishVoiceDictation,
  quoteOf,
  sharedWorkspacePeers,
  type Entry,
} from './models.ts'

test('editing a bot preserves its isolated agent account and older hosts use the default', () => {
  const bot = normalizeBot({ id: 'b', backend: 'antigravity', account: 'work-account' })
  assert.equal(draftOf(bot).account, 'work-account')
  assert.equal(draftOf(normalizeBot({ id: 'legacy' })).account, null)
})

test('a bot keeps its Fast mode through the settings draft', () => {
  const bot = normalizeBot({ id: 'b', backend: 'codex', fastMode: true })
  assert.equal(bot.fastMode, true)
  assert.equal(draftOf(bot).fastMode, true)
})

test('a host without Fast mode reads as off', () => {
  assert.equal(normalizeBot({ id: 'b' }).fastMode, false)
})

test('the composer offers Fast mode until the agent says it has none, and while it is on', async () => {
  const { fastModeShown, fastModeState, fastModeDetail } = await import('./fast-mode.ts')
  assert.equal(fastModeShown(normalizeBot({ id: 'b' })), true)
  assert.equal(fastModeShown(normalizeBot({ id: 'b', fastModeSupport: 'unsupported' })), false)
  assert.equal(fastModeShown(normalizeBot({ id: 'b', fastModeSupport: 'offered' })), true)
  assert.equal(fastModeShown(normalizeBot({ id: 'b', fastMode: true, fastModeSupport: 'unsupported' })), true)
  assert.equal(fastModeShown(normalizeBot({ id: 'g', kind: 'group', fastModeSupport: 'offered' })), false)
  const refused = normalizeBot({ id: 'b', fastMode: true, fastModeSupport: 'refused', fastModeIssue: 'requires extra usage' })
  assert.equal(fastModeState(refused), 'refused')
  assert.match(fastModeDetail(refused), /requires extra usage/)
})

test('memory lines and summaries stay out of the chat', () => {
  const notice = (data: Entry['data']) => ({ id: 'n', botId: 'b', kind: 'notice', rev: 1, turn: 1, createdAt: 0, data }) as Entry
  assert.equal(isChat(notice({ text: 'Remembered: likes tea', memory: { status: 'done' } })), false)
  assert.equal(isChat(notice({ text: 'Updated people.md', memoryFile: { status: 'done' } })), false)
  assert.equal(isChat(notice({ text: 'Earlier context was summarized to make room.', compaction: { byHost: true } })), false)
  assert.equal(isChat(notice({ text: 'Stopped.' })), true)
})

test('a kept handoff leaves no line; a fresh conversation does', () => {
  const notice = (status: string) => ({ id: 'n', botId: 'b', kind: 'notice', rev: 1, turn: 1, createdAt: 0, data: { text: '…', handoff: { status } } }) as Entry
  assert.equal(isChat(notice('kept')), false)
  assert.equal(isChat(notice('sent')), true)
  assert.equal(isChat(notice('draft')), true)
})

const message = (kind: string, data: Entry['data'], id = 'e1'): Entry => ({ id, seq: 1, botId: 'b', rev: 1, kind, turn: 1, createdAt: 0, updatedAt: 0, data })

test('the user and final bot replies can be quoted, once the computer has them', () => {
  assert.equal(isReplyable(message('user', { text: 'hi' })), true)
  assert.equal(isReplyable(message('agent', { text: 'done', final: true })), true)
  assert.equal(isReplyable(message('agent', { text: 'narration' })), false)
  assert.equal(isReplyable(message('notice', { text: 'Stopped.' })), false)
  assert.equal(isReplyable(message('permission', { title: 'Run?' })), false)
  assert.equal(isReplyable(message('user', { text: 'sending' }, 'local-n')), false)
})

test('a quote names who wrote it and falls back to file names', () => {
  assert.deepEqual(quoteOf(message('user', { text: 'hi' })), { id: 'e1', author: 'user', text: 'hi' })
  assert.equal(quoteOf(message('agent', { text: 'yo', final: true, author: 'ann' })).author, 'ann')
  assert.equal(quoteOf(message('agent', { text: 'yo', final: true })).author, 'b')
  assert.equal(quoteOf(message('user', { text: '', attachments: [{ id: 'a', name: 'a.png', size: 1 }, { id: 'c', name: 'c.pdf', size: 1 }] })).text, 'a.png, c.pdf')
  assert.equal(quoteOf(message('user', { text: 'x'.repeat(900) })).text.length, 500)
})

test('a bot preserves robot avatarLook, 7 actionRules and proactive settings through draftOf', () => {
  assert.equal(ACTION_CLASSES.length, 7)
  assert.equal(BOT_AVATAR_LOOKS.length, 12)
  const proactive = { ...defaultProactiveSettings(), enabled: true, everyHours: 2 }
  const bot = normalizeBot({
    id: 'b',
    avatarLook: 'visor',
    actionRules: { shell: 'block', files: 'allow' },
    proactive,
  })
  assert.equal(bot.avatarLook, 'visor')
  assert.deepEqual(bot.actionRules, { shell: 'block', files: 'allow' })
  assert.equal(bot.proactive?.enabled, true)
  const draft = draftOf(bot)
  assert.equal(draft.avatarLook, 'visor')
  assert.deepEqual(draft.actionRules, { shell: 'block', files: 'allow' })
  assert.equal(draft.proactive?.everyHours, 2)
})

test('sharedWorkspacePeers detects simultaneous active bots in the same project across path separators', () => {
  const b1 = normalizeBot({ id: '1', name: 'Coder', cwd: 'C:\\Users\\Edwin\\Repo\\', status: 'working' })
  const b2 = normalizeBot({ id: '2', name: 'Reviewer', cwd: 'c:/users/edwin/repo', status: 'needsInput' })
  const b3 = normalizeBot({ id: '3', name: 'IdleBot', cwd: 'c:/users/edwin/repo', status: 'idle' })
  const b4 = normalizeBot({ id: '4', name: 'PersonalBot', cwd: 'c:/users/edwin/repo', managedWorkspace: true, status: 'working' })
  const peers = sharedWorkspacePeers(b1, [b1, b2, b3, b4])
  assert.deepEqual(peers.map((p) => p.id), ['2'])
})

test('collectPendingDecisions, collectDeliverables, WORKER_ROLE_TEMPLATES and polishVoiceDictation work as specified', () => {
  const entries = [
    message('permission', { title: 'Run git push?', text: 'git push origin dev', status: 'pending', options: [{ optionId: 'yes', name: 'Allow', kind: 'allow_once' }, { optionId: 'no', name: 'Reject', kind: 'reject_once' }] }, 'p1'),
    message('permission', { title: 'Old permission', status: 'approved' }, 'p2'),
    message('agent', {
      text: 'Generated report',
      files: [{ id: 'f1', name: 'report.pdf', size: 20480, sha256: 'abcdef1234567890' }],
    }, 'a1'),
  ]
  const decisions = collectPendingDecisions(entries)
  assert.equal(decisions.length, 1)
  assert.equal(decisions[0]?.entryId, 'p1')
  assert.equal(decisions[0]?.kind, 'permission')
  assert.equal(decisions[0]?.allowOptionId, 'yes')
  assert.equal(decisions[0]?.denyOptionId, 'no')

  const deliverables = collectDeliverables(entries)
  assert.equal(deliverables.length, 1)
  assert.equal(deliverables[0]?.name, 'report.pdf')

  assert.equal(WORKER_ROLE_TEMPLATES.length, 5)
  assert.equal(
    polishVoiceDictation('  um, revisa el archivo ,por favor.   uh gracias! '),
    'Revisa el archivo, por favor. Gracias!',
  )
})

test('collectPendingDecisions reads takeovers and unanswered errors from the entries the host writes', () => {
  const handoff = message('notice', { text: 'Human takeover requested: log in', takeoverRequest: { reason: 'log in', status: 'waiting' } }, 'h')
  const done = message('notice', { text: 'x', takeoverRequest: { reason: 'old', status: 'resolved' } }, 'hd')
  const lapsed = message('notice', { text: 'x', takeoverRequest: { reason: 'late', status: 'expired' } }, 'hx')
  const failure = message('notice', { text: 'The agent stopped', style: 'error' }, 'e')
  const ids = (list: ReturnType<typeof collectPendingDecisions>) => list.map((d) => d.entryId).sort()
  assert.deepEqual(ids(collectPendingDecisions([handoff, done, lapsed, failure])), ['e', 'h'])
  const reply = { ...message('agent', { text: 'Back on it' }, 'a'), seq: failure.seq + 1 }
  assert.deepEqual(ids(collectPendingDecisions([handoff, done, failure, reply])), ['h'])
})
