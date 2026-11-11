import { test, expect } from '@playwright/test'
import { readFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { api, attach, sandbox, typeMessage } from './sandbox.ts'

// An agent without session/close, like Cursor, whose every session holds a worker as an MCP server would
// (host/tests/fixtures/resource_agent.py). Playwright loads specs as CommonJS here, so `__dirname` is defined.
const agent = join(resolve(__dirname, '../../..'), 'host/tests/fixtures/resource_agent.py')

test('routine runs on an agent that cant close sessions restart it, and the chat keeps its conversation', async () => {
  test.skip(process.platform === 'win32', "on Windows a worker that isn't Codync's keeps its agent running")
  const { browser, page } = await attach()
  const name = `Resources e2e ${Date.now()}`
  let botId: string | undefined
  try {
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command: `python3 -X utf8 -u '${agent}' --no-close`, connectors: [], skills: [], notify: false,
    })
    botId = bot.id
    await page.locator('.roster-row', { hasText: name }).click()
    const transcript = page.locator('.transcript')
    await typeMessage(page, 'Remember this conversation')
    await expect(transcript).toContainText('Resource fixture replied.')
    // Two routines due at once: the second waits in the queue while the first one's session stays open.
    for (const routine of ['First run', 'Second run']) {
      const saved = await api<{ routine: { id: string } }>(page, 'saveRoutine', {
        botId, name: routine, instruction: 'Report once', triggers: [{ type: 'webhook' }], timeoutSeconds: 30,
      })
      await api(page, 'runRoutine', { botId, id: saved.routine.id })
    }
    await expect.poll(async () => {
      const listing = await api<{ runs: { status: string }[] }>(page, 'routines', { botId })
      return listing.runs.filter((run) => run.status === 'succeeded').length
    }, { timeout: 20_000 }).toBe(2)
    await typeMessage(page, 'Continue the same conversation')
    await expect.poll(async () => {
      const history = await api<{ entries: { kind: string; data: { final?: boolean } }[] }>(page, 'history', { botId })
      return history.entries.filter((entry) => entry.kind === 'agent' && entry.data.final).length
    }).toBe(4)
    await expect(transcript).toContainText('Continue the same conversation')
    const log = join(sandbox().home, 'bots', botId, 'workspace/resources.jsonl')
    const rows = readFileSync(log, 'utf8').trim().split('\n').map((line) => JSON.parse(line))
    const chat = rows.filter((row) => row.event === 'prompt' && !String(row.text).includes('Report once'))
    expect(chat.at(-1).session).toBe(chat[0].session)
    expect(rows.filter((row) => row.event === 'started').length).toBeGreaterThan(1)
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
