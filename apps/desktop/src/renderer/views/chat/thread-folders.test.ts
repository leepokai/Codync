import assert from 'node:assert/strict'
import { test } from 'node:test'
import { defaultFolder, threadFolders } from './thread-folders.ts'

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
