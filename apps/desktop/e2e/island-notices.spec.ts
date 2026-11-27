import { expect, test, type Page } from '@playwright/test'
import { resolve } from 'node:path'
import { fixtureCommand } from './fixture-command.ts'
import { api, attach, withPrefs } from './sandbox.ts'

const agent = resolve(__dirname, '../../../host/tests/fixtures/send_cli_agent.py')

const hasBot = (island: Page, botId: string) => island.evaluate(async (botId) => {
  const { summary } = await window.codync.island.state()
  return summary.bots.some((bot) => bot.ref.endsWith('/' + botId))
}, botId)

test('showing a hidden bot again does not replay its last completion', async () => {
  const { browser, page } = await attach()
  const name = `Island notice ${Date.now()}`
  const wasEnabled = await page.evaluate(() => window.codync.island.enabled())
  const restoreLanguage = await withPrefs(page, { appLanguage: 'en' })
  let botId: string | undefined
  try {
    test.skip(!(await page.evaluate(() => window.codync.island.supported)), 'This platform has no island window.')
    await page.evaluate(() => window.codync.island.setEnabled(true))
    const islandPage = () => page.context().pages().find((p) => p.url().includes('?window=island'))
    await expect.poll(islandPage).toBeTruthy()
    const island = islandPage()!
    const panel = island.locator('.island')
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command: fixtureCommand(agent), connectors: [], skills: [], notify: false,
    })
    botId = bot.id
    await expect.poll(() => hasBot(island, bot.id)).toBe(true)

    // A hidden island can have zero height. Open it to wait for the renderer's bot summary.
    await island.getByRole('button', { name: 'Show bots', exact: true }).dispatchEvent('click')
    await expect(panel.locator('.island-empty').getByRole('button', { name: 'Message a bot', exact: true })).toBeVisible()
    await panel.getByRole('button', { name: 'Open Codync', exact: true }).click()
    await expect(panel).toHaveAttribute('data-mode', 'hidden')

    const hideAndShow = async () => {
      await api(page, 'updateBot', { id: bot.id, hidden: true })
      await expect.poll(() => hasBot(island, bot.id)).toBe(false)
      await expect(panel).toHaveAttribute('data-mode', 'hidden', { timeout: 1000 })
      await api(page, 'updateBot', { id: bot.id, hidden: false })
      await expect.poll(() => hasBot(island, bot.id)).toBe(true)
      await island.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))))
      await expect(panel).toHaveAttribute('data-mode', 'hidden', { timeout: 1000 })
    }

    await api(page, 'send', { botId, text: 'CLI_MODE=echo CLI_TEXT=First island reply' })
    await expect(panel).toHaveAttribute('data-mode', 'finished')
    await expect(island.locator('.island-alert')).toContainText('First island reply')
    await island.getByRole('button', { name: 'OK', exact: true }).click()
    await expect(panel).toHaveAttribute('data-mode', 'hidden')
    await hideAndShow()

    await api(page, 'send', { botId, text: 'CLI_MODE=echo CLI_TEXT=Second island reply' })
    await expect(panel).toHaveAttribute('data-mode', 'finished')
    await expect(island.locator('.island-alert')).toContainText('Second island reply')
    await hideAndShow()
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await page.evaluate((on) => window.codync.island.setEnabled(on), wasEnabled)
    await restoreLanguage()
    await browser.close()
  }
})
