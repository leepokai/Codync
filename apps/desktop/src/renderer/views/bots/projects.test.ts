import assert from 'node:assert/strict'
import { test } from 'node:test'
import { draftOf, normalizeBot } from '../../../shared/models.ts'
import { withDefaultProject, withProject, withoutProject } from './projects.ts'

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
