import { test, expect, type Page } from '@playwright/test'
import { execSync } from 'node:child_process'
import { api, attach } from './sandbox.ts'

// Temporary (repro branch only): what the computer shows after a dev server's Stop then Start.
const sh = (cmd: string) => { try { return execSync(cmd, { encoding: 'utf8' }) } catch (e) { return String((e as { stdout?: string }).stdout ?? e) } }
async function pick(page: Page, row: ReturnType<Page['locator']>, title: string) {
  await row.hover()
  await row.locator('.dev-server-more').click()
  await page.locator('.menu-row', { hasText: title }).click()
}
test('dev server restart, observed', async () => {
  test.setTimeout(90_000)
  const { browser, page } = await attach()
  const name = `Dev debug ${Date.now()}`
  const port = 20_000 + Math.floor(Math.random() * 10_000)
  const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', { name, backend: 'custom', command: 'unused', connectors: [], permission: 'auto' })
  try {
    await page.locator('.roster-row', { hasText: name }).click()
    await api(page, 'chatCall', { botId: bot.id, name: 'start_dev_server', arguments: { command: `python3 -m http.server ${port} --bind 127.0.0.1`, name: 'web' } })
    const pill = page.locator('.title-pill')
    await expect(pill).toContainText(`:${port}`)
    await pill.click()
    const row = page.locator('.dev-server-row', { hasText: 'web' })
    console.log('T0', sh(`ps -axo pid,ppid,pgid,sess,stat,command | grep -v grep | grep http.server`))
    await pick(page, row, 'Stop')
    await expect(row).toContainText('Stopped')
    console.log('AFTER STOP', sh(`ps -axo pid,ppid,pgid,sess,stat,command | grep -v grep | grep http.server; lsof -nP -iTCP:${port}`))
    await pick(page, row, 'Start')
    for (let i = 0; i < 12; i++) {
      await page.waitForTimeout(1000)
      console.log('T+' + i, (await row.innerText()).replace(/\n/g, ' '), '|', sh(`ps -axo pid,ppid,pgid,sess,stat,command | grep -v grep | grep http.server; lsof -nP -iTCP:${port} -sTCP:LISTEN`))
    }
    await row.hover()
    await row.locator('.dev-server-more').click()
    await page.locator('.menu-row', { hasText: 'Logs' }).click()
    console.log('LOG', await page.locator('.modal-content').innerText())
  } finally {
    await api(page, 'deleteBot', { botId: bot.id }).catch(() => {})
    await page.reload()
    await browser.close()
  }
})
