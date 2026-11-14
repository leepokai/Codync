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

test('a click on the voice note plays or pauses it; only its bar seeks', async () => {
  const { browser, page } = await attach()
  const name = `Voice note play e2e ${Date.now()}`
  let botId: string | undefined
  try {
    await setPref(page, 'appLanguage', 'en')
    const continueWithoutAccount = page.getByRole('button', { name: /^Continue without an account/ })
    if (await continueWithoutAccount.isVisible()) await continueWithoutAccount.click()
    const agent = resolve(__dirname, '../../../host/tests/fixtures/steer_agent.py')
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command: `python3 -X utf8 -u '${agent}' voice-wav 0`,
      connectors: [], skills: [], notify: false,
    })
    botId = bot.id
    await page.locator('.roster-row', { hasText: name }).click()
    await api(page, 'send', { botId, text: 'Send a voice note' })
    const player = page.locator('.voice-note')
    const play = player.getByTitle('Play')
    const pause = player.getByTitle('Pause')
    await expect(player).toBeVisible()
    // The play button plays; a click on the player's clock pauses.
    await play.click()
    await expect(pause).toBeVisible()
    await player.locator('.file-card-detail').click()
    await expect(play).toBeVisible()
    // Beside the bar, above its 12 px box, the click still plays rather than seeking.
    const bar = player.locator('input.file-card-progress')
    const box = (await bar.boundingBox())!
    const playerBox = (await player.boundingBox())!
    await page.mouse.click(box.x + box.width * 0.75, playerBox.y + 1)
    await expect(pause).toBeVisible()
    await pause.click()
    await expect(play).toBeVisible()
    // On the bar itself, a click moves the playhead and leaves the note paused.
    const before = Number(await bar.inputValue())
    await bar.click({ position: { x: box.width * 0.75, y: box.height / 2 } })
    await expect.poll(async () => Number(await bar.inputValue())).toBeGreaterThan(before + 1)
    await expect(play).toBeVisible()
  } finally {
    if (botId) await api(page, 'deleteBot', { botId })
    await browser.close()
  }
})
