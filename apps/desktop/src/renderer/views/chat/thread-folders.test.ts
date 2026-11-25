import assert from 'node:assert/strict'
import { test } from 'node:test'
import { defaultFolder, modePlace, offersPlace, sentSetup, threadFolders } from './thread-folders.ts'

test("a new thread's folders: the projects, the bot's own folder, then recent ones not listed; the default sends none", () => {
  const bot = { cwd: '/home', projects: ['/p/a', '/p/b'] }
  assert.equal(defaultFolder(bot), '/p/a')
  assert.deepEqual(threadFolders(bot, ['/p/b', '/r/x']).map((f) => [f.path, f.home, f.recent, f.cwd]), [
    ['/p/a', false, false, undefined],
    ['/p/b', false, false, '/p/b'],
    ['/home', true, false, '/home'],
    ['/r/x', false, true, '/r/x'],
  ])
  const none = { cwd: '/home', projects: [] }
  assert.deepEqual([defaultFolder(none), threadFolders(none, []).map((f) => f.cwd)], ['/home', [undefined]])
})

/** What the host reads. */
const wire = (s: object) => JSON.parse(JSON.stringify(s)) as Record<string, unknown>

test("a new thread names its place only in one of the bot's git projects, and shows the mode's until picked", () => {
  const bot = { cwd: '/home', projects: ['/p/git', '/p/plain'], projectGitRepos: [true, false] }
  assert.deepEqual(['/p/git', '/p/plain', '/home', '/r/x'].map((p) => offersPlace(bot, p)), [true, false, false, false])
  assert.deepEqual((['off', 'sideBySide', 'always', null] as const).map(modePlace), ['here', null, 'worktree', null])
  const at = { plain: false, home: '/home', hostChooses: false }
  assert.deepEqual(wire(sentSetup({ cwd: '/p/x', place: 'worktree' }, { ...at, offered: true })), { cwd: '/p/x', place: 'worktree' })
  assert.deepEqual(wire(sentSetup({ cwd: '/p/x', place: 'worktree' }, { ...at, offered: false })), { cwd: '/p/x' }, 'a folder outside a git project drops it')
  assert.deepEqual(wire(sentSetup({}, { ...at, offered: true })), {}, 'none picked: the mode decides, and the default folder goes as none')
})

test('a thread without a bot sends no folder where the host makes its own, the home folder to an older host', () => {
  const plain = { offered: false, plain: true, home: '/home' }
  assert.deepEqual(wire(sentSetup({ backend: 'claude' }, { ...plain, hostChooses: true })), { backend: 'claude' })
  assert.deepEqual(wire(sentSetup({ backend: 'claude' }, { ...plain, hostChooses: false })), { backend: 'claude', cwd: '/home' })
  assert.deepEqual(wire(sentSetup({ backend: 'claude', cwd: '/home' }, { ...plain, hostChooses: true })), { backend: 'claude', cwd: '/home' }, 'the home folder picked goes')
  assert.deepEqual(wire(sentSetup({ backend: 'claude', place: 'here' }, { ...plain, hostChooses: true })), { backend: 'claude' })
})
