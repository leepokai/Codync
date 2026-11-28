import { test, expect } from '@playwright/test'
import { join, resolve } from 'node:path'
import { fixtureCommand } from './fixture-command.ts'
import { api, attach } from './sandbox.ts'

// A scripted agent that answers each message with `mode <its mode> · <the message>` (host/tests/fixtures/mode_agent.py).
const echoAgent = join(resolve(__dirname, '../../..'), 'host/tests/fixtures/mode_agent.py')

type Step = { id: string; nodeId: string; status: string }
type Run = { run: { id: string; status: string; steps: Step[] } }

test('a step run on its own shows its result, and an approval on the canvas moves its run on once', async () => {
  const { browser, page } = await attach()
  const stamp = Date.now()
  let botId: string | undefined
  let workflowId: string | undefined
  try {
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name: `Workflow step e2e ${stamp}`, backend: 'custom', command: fixtureCommand(echoAgent), connectors: [], skills: [], notify: false,
    })
    botId = bot.id
    const title = `Approval e2e ${stamp}`
    const { workflow } = await api<{ workflow: { id: string } }>(page, 'createWorkflow', { title })
    workflowId = workflow.id
    const graph = {
      nodes: [
        { id: 't', kind: 'trigger', at: { x: 40, y: 40 }, fields: { trigger: 'manual' } },
        { id: 'a', kind: 'agent', at: { x: 40, y: 300 }, fields: { botId: bot.id, task: 'hello' } },
        { id: 'p', kind: 'approval', at: { x: 400, y: 40 }, fields: { question: 'Ship it?' } },
      ],
      edges: [
        { id: 't>a:text', from: 't', to: 'a', type: 'text' },
        { id: 'a>p:text', from: 'a', to: 'p', type: 'text' },
      ],
    }
    await api(page, 'saveWorkflowGraph', { id: workflow.id, baseRevision: 1, graph })

    await page.locator('.nav-rail [data-page="workflows"]').click()
    await page.locator('aside.sidebar button.thread-row', { hasText: title }).click()
    const node = (id: string) => page.locator(`.workflow-node[data-node="${id}"]`)
    await expect(node('a')).toBeVisible()

    // Run step: the step belongs to no run, so its result reaches the canvas through its own step event.
    await node('a').locator('button.workflow-run').dispatchEvent('click')
    await expect(node('a')).toContainText(/mode \w+ · /, { timeout: 20_000 })
    await expect(node('a')).not.toContainText('Waiting')

    // Run workflow: the approval waits on the canvas until Approve.
    await node('t').locator('button.workflow-run').dispatchEvent('click')
    const approve = node('p').getByRole('button', { name: 'Approve', exact: true })
    await expect(approve).toBeVisible({ timeout: 20_000 })
    await approve.dispatchEvent('click')
    let runId = ''
    await expect.poll(async () => {
      const { runs } = await api<{ runs: { id: string; status: string }[] }>(page, 'workflowRuns', { id: workflow.id, limit: 1 })
      runId = runs[0]?.id ?? ''
      return runs[0]?.status
    }, { timeout: 20_000 }).toBe('done')
    await expect(approve).toHaveCount(0)

    // A late answer from another device (Reject after Approve) changes nothing.
    const { run } = await api<Run>(page, 'workflowRun', { runId })
    const decided = run.steps.find((s) => s.nodeId === 'p')!
    await api(page, 'decideWorkflowStep', { stepId: decided.id, approved: false })
    const after = await api<Run>(page, 'workflowRun', { runId })
    expect(after.run.status).toBe('done')
    expect(after.run.steps.find((s) => s.nodeId === 'p')?.status).toBe('done')
  } finally {
    if (workflowId) await api(page, 'deleteWorkflow', { id: workflowId }).catch(() => {})
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
