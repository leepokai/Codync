import assert from 'node:assert/strict'
import { test } from 'node:test'
import { draftOf, normalizeBot } from '../../../shared/models.ts'
import { sharedFolderName, withDefaultProject, withProject, withSharedFolder, withoutProject, withoutSharedFolder } from './projects.ts'

/** What a save sends, as the host reads it. */
const body = (d: ReturnType<typeof draftOf>) => JSON.parse(JSON.stringify(d)) as Record<string, unknown>

test('a save sends the whole project list, default first, and the worktree mode, not the old switch', () => {
  const bot = normalizeBot({ id: 'b', name: 'B', backend: 'claude', cwd: '/h/bots/b/workspace', managedWorkspace: true, projects: ['/p/a', '/p/b'], projectGitRepos: [true, false], worktreeMode: 'always', threadWorktrees: true })
  const sent = body(draftOf(bot))
  assert.deepEqual([sent.projects, sent.worktreeMode, 'threadWorktrees' in sent, sent.cwd], [['/p/a', '/p/b'], 'always', false, ''])

  const added = withProject(draftOf(bot), '/p/c')
  assert.deepEqual(body(added).projects, ['/p/a', '/p/b', '/p/c'])
  assert.deepEqual(withProject(added, '/p/a').projects, ['/p/a', '/p/b', '/p/c'], 'one already there stays where it is')
  assert.deepEqual(body(withDefaultProject(added, '/p/c')).projects, ['/p/c', '/p/a', '/p/b'])
  assert.deepEqual(body(withoutProject(added, '/p/a')).projects, ['/p/b', '/p/c'])
  assert.deepEqual(body(withoutProject(withoutProject(draftOf(bot), '/p/a'), '/p/b')).projects, [], 'the last one goes as an empty list')
  assert.equal(body({ ...draftOf(bot), worktreeMode: 'off' }).worktreeMode, 'off')
})

test('a host before projects gets neither field, and keeps its switch', () => {
  const old = normalizeBot({ id: 'b', cwd: '/p/a', threadWorktrees: false, gitRepo: true })
  assert.deepEqual([old.projects, old.projectGitRepos, old.worktreeMode], [[], [], null])
  const sent = body(draftOf(old))
  assert.deepEqual(['projects' in sent, 'worktreeMode' in sent, sent.threadWorktrees, sent.cwd], [false, false, false, '/p/a'])
})

test("a save sends every project's shared folders as one map; a project left with none leaves it", () => {
  const bot = normalizeBot({ id: 'b', cwd: '/h', projects: ['/p/a', '/p/b'], projectGitRepos: [true, true], worktreeMode: 'off', sharedFolders: { '/p/a': ['node_modules'] } })
  const added = withSharedFolder(withSharedFolder(draftOf(bot), '/p/b', '.cache/pip'), '/p/a', 'node_modules')
  assert.deepEqual(body(added).sharedFolders, { '/p/a': ['node_modules'], '/p/b': ['.cache/pip'] }, 'one already there stays once')
  assert.deepEqual(body(withoutSharedFolder(added, '/p/a', 'node_modules')).sharedFolders, { '/p/b': ['.cache/pip'] })
  assert.deepEqual(body(withoutSharedFolder(withoutSharedFolder(added, '/p/a', 'node_modules'), '/p/b', '.cache/pip')).sharedFolders, {}, 'the last one goes as an empty map')
  assert.equal('sharedFolders' in body(draftOf(normalizeBot({ id: 'b', cwd: '/h', projects: ['/p/a'], worktreeMode: 'off' }))), false, 'a host before shared folders gets none')
})

test('a typed shared folder is a plain path inside the project, or nothing', () => {
  assert.deepEqual(['node_modules', ' ./.cache/pip/ ', 'a\\b'].map(sharedFolderName), ['node_modules', '.cache/pip', 'a/b'])
  assert.deepEqual(['', '  ', '.', '/abs', 'C:\\x', '\\x', '../up', 'a/../b'].map(sharedFolderName), Array(8).fill(null))
})
