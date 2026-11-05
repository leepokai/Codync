import { test, expect } from '@playwright/test'
import { readFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { join, resolve } from 'node:path'
import { api, attach, sandbox } from './sandbox.ts'

test('a warm conversation survives queued routines while old connector allocations are retired', async () => {
  const { browser, page } = await attach()
  const name = `Resources e2e ${Date.now()}`
  const fixture = resolve(__dirname, '../../../host/tests/fixtures/resource_agent.py')
  let botId: string | undefined
  try {
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command: `python3 -X utf8 -u '${fixture}' --no-close`,
      connectors: [], skills: [], notify: false,
    })
    botId = bot.id
    await page.locator('.roster-row', { hasText: name }).click()
    const transcript = page.locator('.transcript')
    const composer = page.locator('.composer textarea')
    await composer.fill('Remember this conversation')
    await composer.press('Enter')
    await expect(transcript).toContainText('Resource fixture replied.')
    const { routine } = await api<{ routine: { id: string } }>(page, 'saveRoutine', {
      botId, name: 'Resource run', instruction: 'Report once', triggers: [{ type: 'webhook' }], timeoutSeconds: 30,
    })
    for (let n = 0; n < 3; n++) await api(page, 'runRoutine', { botId, id: routine.id })
    await expect.poll(async () => {
      const listing = await api<{ runs: { status: string }[] }>(page, 'routines', { botId })
      return listing.runs.filter((run) => run.status === 'succeeded').length
    }, { timeout: 20_000 }).toBe(3)
    await composer.fill('Continue the same conversation')
    await composer.press('Enter')
    await expect.poll(async () => {
      const history = await api<{ entries: { kind: string; data: { final?: boolean } }[] }>(page, 'history', { botId })
      return history.entries.filter((entry) => entry.kind === 'agent' && entry.data.final).length
    }).toBe(5)
    const log = join(homedir(), `.codync-dev-${sandbox().name}`, 'bots', botId, 'workspace/resources.jsonl')
    const rows = readFileSync(log, 'utf8').trim().split('\n').map((line) => JSON.parse(line))
    const prompts = rows.filter((row) => row.event === 'prompt')
    expect(prompts.at(-1).session).toBe(prompts[0].session)
    expect(rows.filter((row) => row.event === 'started').length).toBeGreaterThan(1)
    await expect(transcript).toContainText('Continue the same conversation')
  } finally {
    if (botId) await api(page, 'deleteBot', { botId })
    await browser.close()
  }
})
