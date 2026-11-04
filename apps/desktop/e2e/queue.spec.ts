import { test, expect, type Page } from '@playwright/test'
import { readFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { join, resolve } from 'node:path'
import { SANDBOX_PREFIX } from '../src/shared/dev-sandbox.ts'
import { canSteer } from '../src/renderer/views/thread/queued-messages.ts'
import { attach, sandbox } from './sandbox.ts'

// A scripted agent whose turn waits until a steered message joins it (host/tests/fixtures/steer_agent.py).
const agent = resolve(__dirname, '../../../host/tests/fixtures/steer_agent.py')

async function host<T = Record<string, unknown>>(method: string, body: object): Promise<T> {
  const { name, port } = sandbox()
  const token = readFileSync(join(homedir(), SANDBOX_PREFIX + name, 'token'), 'utf8').trim()
  const res = await fetch(`http://127.0.0.1:${port}/api/${method}`, {
    method: 'POST',
    headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`${method}: ${await res.text()}`)
  return (await res.json()) as T
}

/** Whether the turn answering `text` has started. */
async function started(botId: string, text: string): Promise<boolean> {
  const { entries } = await host<{ entries: { kind: string; data: { text?: string; status?: string } }[] }>('history', { botId })
  return entries.some((e) => e.kind === 'user' && e.data.text === text && e.data.status === 'sent')
}

async function type(page: Page, text: string) {
  await page.locator('.composer textarea').fill(text)
  await page.locator('.composer textarea').press('Enter')
}

test('a message sent while the bot works waits above the composer, then goes out now or is taken back', async () => {
  const { version } = await host<{ version: string }>('hello', {})
  test.skip(!canSteer(version), `the sandbox host (${version}) can't steer yet`)
  const { browser, page } = await attach()
  const name = `Queue e2e ${Date.now()}`
  const { bot } = await host<{ bot: { id: string } }>('createBot', {
    name, backend: 'custom', command: `python3 -X utf8 -u '${agent}' steer 30`, connectors: [], skills: [], notify: false,
  })
  try {
    await page.locator('.roster-row', { hasText: name }).click()
    const transcript = page.locator('.transcript')
    const queued = page.locator('.queued-row')

    await host('send', { botId: bot.id, text: 'first' })
    await expect.poll(() => started(bot.id, 'first')).toBe(true)
    await type(page, 'second')
    await expect(queued.filter({ hasText: 'second' })).toBeVisible()
    await expect(transcript).not.toContainText('second')
    await queued.filter({ hasText: 'second' }).getByRole('button', { name: 'Send now' }).click()
    // It joined the running turn: the chat shows it, and the reply took it in.
    await expect(queued).toHaveCount(0)
    await expect(transcript).toContainText('second')
    await expect(transcript).toContainText('Got: ')

    await host('send', { botId: bot.id, text: 'third' })
    await expect.poll(() => started(bot.id, 'third')).toBe(true)
    await type(page, 'never sent')
    await queued.filter({ hasText: 'never sent' }).getByRole('button', { name: 'Remove' }).click()
    await expect(queued).toHaveCount(0)
    await expect(transcript).not.toContainText('never sent')
    const { entries } = await host<{ entries: { data: { text?: string; removed?: boolean } }[] }>('history', { botId: bot.id })
    expect(entries.find((e) => e.data.text === 'never sent')?.data.removed).toBe(true)
  } finally {
    await host('stop', { botId: bot.id }).catch(() => {})
    await host('deleteBot', { botId: bot.id }).catch(() => {})
    await browser.close()
  }
})
