import { test, expect, type Page } from '@playwright/test'
import { join, resolve } from 'node:path'
import { fixtureCommand } from './fixture-command.ts'
import { api, attach } from './sandbox.ts'

// A scripted agent that answers each message with `mode <its mode> · <the message>` (host/tests/fixtures/mode_agent.py).
const echoAgent = join(resolve(__dirname, '../../..'), 'host/tests/fixtures/mode_agent.py')

type Step = { id: string; nodeId: string; status: string }
type Run = { run: { id: string; status: string; error?: string | null; steps: Step[] } }
type Node = { id: string; kind: string; at: { x: number; y: number }; fields: Record<string, string> }

const at = (i: number) => ({ x: 40 + 300 * i, y: 40 })
const trigger = (id: string, i: number): Node => ({ id, kind: 'trigger', at: at(i), fields: { trigger: 'manual' } })
const agent = (id: string, i: number, botId: string): Node => ({ id, kind: 'agent', at: at(i), fields: { botId, task: 'hi' } })
const line = (from: string, to: string, branch?: 'yes' | 'no') =>
  ({ id: `${from}${branch ? `.${branch}` : ''}>${to}:text`, from, to, type: 'text', ...(branch ? { branch } : {}) })

/** Makes a workflow with this graph, runs it (from one trigger when given) and returns the run once it settles. */
async function runOf(page: Page, made: string[], nodes: Node[], edges: unknown[], from?: string, settle?: (read: () => Promise<Run['run']>) => Promise<void>) {
  const { workflow } = await api<{ workflow: { id: string } }>(page, 'createWorkflow', { title: `Run outcome e2e ${Date.now()}` })
  made.push(workflow.id)
  await api(page, 'saveWorkflowGraph', { id: workflow.id, baseRevision: 1, graph: { nodes, edges } })
  const started = await api<Run>(page, 'startWorkflowRun', { id: workflow.id, requestId: `e2e-${Date.now()}`, ...(from ? { fromNodeId: from } : {}), input: '' })
  const runId = started.run.id
  const read = () => api<Run>(page, 'workflowRun', { runId })
  if (settle) await settle(async () => (await read()).run)
  await expect.poll(async () => (await read()).run.status, { timeout: 20_000 }).not.toMatch(/running|approval/)
  return (await read()).run
}

test('a run follows only the branch its condition took, starts a step shared with a trigger it did not start from, and ends on each step\'s latest attempt', async () => {
  const { browser, page } = await attach()
  const made: string[] = []
  let botId = ''
  try {
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name: `Workflow outcome e2e ${Date.now()}`, backend: 'custom', command: fixtureCommand(echoAgent), connectors: [], skills: [], notify: false,
    })
    botId = bot.id
    // Trigger → Draft → Check; Check's No loops back to Draft, Yes goes on to Ship. The reply passes Check, so Draft runs once.
    const looped = await runOf(page, made, [
      trigger('t', 0), agent('a', 1, botId),
      { id: 'c', kind: 'condition', at: at(2), fields: { op: 'contains', value: 'mode' } },
      agent('o', 3, botId),
    ], [line('t', 'a'), line('a', 'c'), line('c', 'a', 'no'), line('c', 'o', 'yes')])
    expect(looped.status).toBe('done')
    expect(looped.steps.filter((s) => s.nodeId === 'a')).toHaveLength(1)
    expect(looped.steps.filter((s) => s.nodeId === 'o').map((s) => s.status)).toEqual(['done'])

    // Two triggers feed one step; a run from the first still runs it.
    const shared = await runOf(page, made, [trigger('t1', 0), trigger('t2', 1), agent('a', 2, botId)], [line('t1', 'a'), line('t2', 'a')], 't1')
    expect(shared.status).toBe('done')
    expect(shared.steps.filter((s) => s.nodeId === 'a').map((s) => s.status)).toEqual(['done'])

    // An approval stopped, then retried and approved: the run ends done, not stopped.
    const retried = await runOf(page, made, [trigger('t', 0), { id: 'p', kind: 'approval', at: at(1), fields: { question: 'Ship it?' } }], [line('t', 'p')], undefined, async (step) => {
      await expect.poll(async () => (await step()).steps.find((s) => s.nodeId === 'p')?.status, { timeout: 20_000 }).toBe('approval')
      const first = (await step()).steps.find((s) => s.nodeId === 'p')!
      await api(page, 'stopWorkflowStep', { stepId: first.id })
      await expect.poll(async () => (await step()).status, { timeout: 20_000 }).toBe('stopped')
      await api(page, 'retryWorkflowStep', { stepId: first.id })
      await expect.poll(async () => (await step()).steps.filter((s) => s.nodeId === 'p' && s.status === 'approval').length, { timeout: 20_000 }).toBe(1)
      const again = (await step()).steps.find((s) => s.nodeId === 'p' && s.status === 'approval')!
      await api(page, 'decideWorkflowStep', { stepId: again.id, approved: true })
    })
    expect(retried.steps.filter((s) => s.nodeId === 'p').map((s) => s.status)).toEqual(['stopped', 'done'])
    expect(retried.status).toBe('done')
  } finally {
    for (const id of made) await api(page, 'deleteWorkflow', { id }).catch(() => {})
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
