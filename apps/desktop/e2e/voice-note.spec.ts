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
    // The error takes the bar's place, centred in it: no bar, the same gap on both sides of the text.
    const error = await player.evaluate((note) => {
      const box = (selector: string) => note.querySelector(selector)!.getBoundingClientRect()
      const line = note.querySelector('.voice-note-error')!
      const range = document.createRange()
      range.selectNodeContents(line)
      const ink = range.getBoundingClientRect()
      return {
        bar: note.querySelector('input.file-card-progress') !== null,
        left: Math.round(ink.left - box('.file-card-play').right),
        right: Math.round(note.getBoundingClientRect().right - ink.right),
      }
    })
    expect(error.bar).toBe(false)
    expect(Math.abs(error.left - error.right)).toBeLessThanOrEqual(1)
    await player.click({ button: 'right' })
    await expect(page.getByRole('menuitem')).toHaveText(['Play', 'Download', 'Reply', 'Show what it did'])
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
    // The sound preloaded as the card came on screen: its bar and clock show before the first play.
    const bar = player.locator('input.file-card-progress')
    await expect(bar).toBeVisible()
    await expect(player.locator('.file-card-detail')).toContainText(' / ')
    // Dragged before the first play, the bar keeps its position: the note opens and starts there.
    const seekBox = (await bar.boundingBox())!
    await bar.click({ position: { x: seekBox.width * 0.25, y: seekBox.height / 2 } })
    await expect.poll(async () => Number(await bar.inputValue())).toBeGreaterThan(0.5)
    await expect(play).toBeVisible()
    await play.click()
    await expect(pause).toBeVisible()
    await expect(player.locator('.file-card-detail')).toContainText(/0:0[1-2] \/ 0:04/)
    await pause.click()
    await expect(play).toBeVisible()
    await page.mouse.move(0, 0)
    // Hovering lays a soft round chip behind the outline glyph over the app's hover time, never a sudden switch.
    const chip = async () => play.evaluate((el) => {
      const style = getComputedStyle(el, '::before')
      return { background: style.backgroundColor, radius: style.borderRadius, transition: style.transitionDuration }
    })
    await expect.poll(async () => (await chip()).background).toBe('rgba(0, 0, 0, 0)')
    await play.hover()
    await expect.poll(async () => (await chip()).background).not.toBe('rgba(0, 0, 0, 0)')
    expect(await chip()).toMatchObject({ radius: '50%', transition: '0.12s' })
    await page.mouse.move(0, 0)
    await expect.poll(async () => (await chip()).background).toBe('rgba(0, 0, 0, 0)')
    // The play button plays; a click on the player's clock pauses.
    await play.click()
    await expect(pause).toBeVisible()
    // The bar sits 10 px from the play glyph's slot and 10 px from the clock.
    const gaps = await player.evaluate((note) => {
      const box = (selector: string) => note.querySelector(selector)!.getBoundingClientRect()
      const track = box('input.file-card-progress')
      return [track.left - box('.file-card-play').right, box('.file-card-detail').left - track.right].map(Math.round)
    })
    expect(gaps).toEqual([10, 10])
    await player.locator('.file-card-detail').click()
    await expect(play).toBeVisible()
    // Beside the bar, above its 12 px box, the click still plays rather than seeking.
    const box = (await bar.boundingBox())!
    const playerBox = (await player.boundingBox())!
    await page.mouse.click(box.x + box.width * 0.75, playerBox.y + 1)
    await expect(pause).toBeVisible()
    await pause.click()
    await expect(play).toBeVisible()
    // On the bar itself, a click moves the playhead to where it landed and leaves the note paused.
    // Compared with the destination, not the playhead before: playback may have moved it meanwhile.
    await bar.click({ position: { x: box.width * 0.75, y: box.height / 2 } })
    await expect.poll(async () => Number(await bar.inputValue())).toBeGreaterThan(2)
    await expect(play).toBeVisible()
  } finally {
    if (botId) await api(page, 'deleteBot', { botId })
    await browser.close()
  }
})
