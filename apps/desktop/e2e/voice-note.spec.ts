import { test, expect } from '@playwright/test'
import { resolve } from 'node:path'
import { api, attach, setPref } from './sandbox.ts'

test('a failed voice note offers retry in the selected language', async () => {
  const { browser, page } = await attach()
  const name = `Voice note e2e ${Date.now()}`
  let botId: string | undefined
  try {
    await setPref(page, 'appLanguage', 'en')
    const continueWithoutAccount = page.getByRole('button', { name: /^Continue without an account/ })
    if (await continueWithoutAccount.isVisible()) await continueWithoutAccount.click()
    await setPref(page, 'appLanguage', 'fr')
    const agent = resolve(__dirname, '../../../host/tests/fixtures/steer_agent.py')
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command: `python3 -X utf8 -u '${agent}' voice 0`,
      connectors: [], skills: [], notify: false,
    })
    botId = bot.id
    await page.locator('.roster-row', { hasText: name }).click()
    await api(page, 'send', { botId, text: 'Send a voice note' })
    const player = page.locator('.voice-note')
    await expect(player).toBeVisible()
    await player.click()
    await expect(player).toContainText('Lecture impossible. Cliquez pour réessayer.')
    await player.click()
    await expect(player).toContainText('Lecture impossible. Cliquez pour réessayer.')
    await setPref(page, 'appLanguage', 'en')
    await expect(player).toContainText("Couldn't play. Click to retry.")
    await player.click({ button: 'right' })
    await expect(page.getByRole('menuitem')).toHaveText(['Play', 'Download', 'Copy', 'Reply', 'Show what it did'])
    await page.keyboard.press('Escape')
  } finally {
    await setPref(page, 'appLanguage', 'en')
    if (botId) await api(page, 'deleteBot', { botId })
    await browser.close()
  }
})
