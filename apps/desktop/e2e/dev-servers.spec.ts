import { test, expect, type Page } from '@playwright/test'
import { mkdirSync, rmSync } from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'
import { SANDBOX_PREFIX } from '../src/shared/dev-sandbox.ts'
import { api, attach, sandbox } from './sandbox.ts'

/** A port nothing listens on yet, below the range the system picks from. */
const freePort = () => 20_000 + Math.floor(Math.random() * 10_000)

/** A bot of its own, its chat open, whose agent never runs: the servers come through the host's API. */
async function openBot(page: Page, name: string) {
  await page.reload()
  const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', { name, backend: 'custom', command: 'unused', connectors: [], permission: 'auto' })
  await page.locator('.roster-row', { hasText: name }).click()
  return bot.id
}

const start = (page: Page, botId: string, command: string, name: string) =>
  api<{ id: string }>(page, 'chatCall', { botId, name: 'start_dev_server', arguments: { command, name } })

async function pick(page: Page, row: ReturnType<Page['locator']>, title: string) {
  await row.hover()
  await row.locator('.dev-server-more').click()
  await page.locator('.menu-row', { hasText: title }).click()
}

test('a dev server shows its port in the title, and the details open, log, stop, start and remove it', async () => {
  const { browser, page } = await attach()
  const name = `Dev server e2e ${Date.now()}`
  const port = freePort()
  let botId: string | undefined
  try {
    botId = await openBot(page, name)
    const pill = page.locator('.title-pill')
    await expect(pill).toContainText(name)
    await expect(pill).not.toContainText(`:${port}`)

    await start(page, botId, `python3 -m http.server ${port} --bind 127.0.0.1`, 'web')
    await expect(pill).toContainText(`:${port}`)

    await pill.click()
    const row = page.locator('.dev-server-row', { hasText: 'web' })
    await expect(row).toContainText(`:${port}`)

    await row.hover()
    await row.locator('.dev-server-more').click()
    await expect(page.getByText('Copy address')).toBeVisible()
    await page.locator('.menu-row', { hasText: 'Logs' }).click()
    await expect(page.locator('.modal-content')).toContainText('python3 -m http.server')
    await page.keyboard.press('Escape')
    await expect(page.locator('.modal-content')).toBeHidden()

    await pick(page, row, 'Stop')
    await expect(row).toContainText('Stopped')
    await expect(pill).not.toContainText(`:${port}`)

    await pick(page, row, 'Start')
    await expect(row).toContainText(`:${port}`)
    await expect(pill).toContainText(`:${port}`)

    await pick(page, row, 'Stop')
    await expect(row).toContainText('Stopped')
    await pick(page, row, 'Remove')
    await expect(page.locator('.dev-server-row')).toHaveCount(0)
    await expect(page.getByText('Dev servers')).toBeHidden()
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})

test('a failing command shows Failed to start, and its row opens the log with how it ended', async () => {
  const { browser, page } = await attach()
  let botId: string | undefined
  try {
    botId = await openBot(page, `Dev server failure e2e ${Date.now()}`)
    await start(page, botId, 'echo no such script; exit 3', 'docs')
    const pill = page.locator('.title-pill')
    await expect(pill).toContainText('Failed to start')

    await pill.click()
    const row = page.locator('.dev-server-row', { hasText: 'docs' })
    await expect(row).toContainText('Failed to start')
    await row.locator('.dev-server-open').click()
    const logs = page.locator('.modal-content')
    await expect(logs).toContainText('Exit code 3')
    await expect(logs).toContainText('no such script')
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})

test("a log that can't be read says why, and a missing one reads as no output", async () => {
  const { browser, page } = await attach()
  let botId: string | undefined
  try {
    botId = await openBot(page, `Dev server log e2e ${Date.now()}`)
    const { id } = await start(page, botId, 'echo finished', 'docs')
    await page.locator('.title-pill').click()
    const row = page.locator('.dev-server-row', { hasText: 'docs' })
    await expect(row).toContainText('Stopped')

    // A folder in its place: reading it fails on every system, unlike file permissions.
    const log = join(homedir(), SANDBOX_PREFIX + sandbox().name, 'dev-servers', `${id}.log`)
    rmSync(log)
    mkdirSync(log)
    await pick(page, row, 'Logs')
    const logs = page.locator('.modal-content')
    await expect(logs).toContainText('reading')
    await expect(logs).not.toContainText('No output yet')

    rmSync(log, { recursive: true })
    await logs.getByRole('button', { name: 'Try again' }).click()
    await expect(logs).toContainText('No output yet')
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
