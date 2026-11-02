import assert from 'node:assert/strict'
import { test } from 'node:test'
import { GraphSaver, type SaveResponse } from './save.ts'
import { emptyGraph, type Graph, type WorkflowDetail } from './workflow.ts'

function detail(revision: number, graph: Graph): WorkflowDetail {
  return { id: 'w', title: 'Workflow', revision, enabled: true, updatedAt: 1, createdAt: 1, graph }
}

function deferred() {
  let resolve: (value: SaveResponse) => void = () => {}
  let reject: (error: unknown) => void = () => {}
  const promise = new Promise<SaveResponse>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

test('a save in flight sends the later graph with the new revision', async () => {
  const bases: number[] = []
  const waiting: ReturnType<typeof deferred>[] = []
  const saver = new GraphSaver(1, (base) => {
    bases.push(base)
    const gate = deferred()
    waiting.push(gate)
    return gate.promise
  })
  const first = emptyGraph()
  const second: Graph = { ...emptyGraph(), nodes: [{ id: 'n', kind: 'trigger', at: { x: 8, y: 8 }, locked: false, fields: { trigger: 'manual' } }] }
  saver.mark(first)
  const pending = saver.flush(first)
  await Promise.resolve()
  saver.mark(second)
  assert.equal((await saver.flush(second)).status, 'queued')
  waiting[0]!.resolve({ ok: true, revision: 2, workflow: detail(2, first) })
  await Promise.resolve()
  await Promise.resolve()
  assert.deepEqual(bases, [1, 2])
  waiting[1]!.resolve({ ok: true, revision: 3, workflow: detail(3, second) })
  const saved = await pending
  assert.equal(saved.status, 'saved')
  assert.equal(saver.revision, 3)
  assert.equal(saver.dirty, false)
})

test('a conflict replaces the revision and clears the dirty flag', async () => {
  const gate = deferred()
  const saver = new GraphSaver(1, () => gate.promise)
  const local = emptyGraph()
  saver.mark(local)
  const pending = saver.flush(local)
  const server = detail(9, { ...emptyGraph(), nodes: [{ id: 'remote', kind: 'trigger', at: { x: 0, y: 0 }, locked: false, fields: { trigger: 'manual' } }] })
  gate.resolve({ ok: false, conflict: true, workflow: server })
  const result = await pending
  assert.equal(result.status, 'conflict')
  assert.equal(saver.dirty, false)
  assert.equal(saver.revision, 9)
})

test('a failed save stays dirty', async () => {
  const gate = deferred()
  const saver = new GraphSaver(4, () => gate.promise)
  saver.mark(emptyGraph())
  const pending = saver.flush(emptyGraph())
  gate.reject(new Error('offline'))
  const result = await pending
  assert.equal(result.status, 'failed')
  assert.equal(saver.dirty, true)
  assert.equal(saver.revision, 4)
})
