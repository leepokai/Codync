import { test, expect } from '@playwright/test'
import { api, attach, setPref } from './sandbox.ts'

test('in Italian a routine row, its editor, the preview and the run settings read translated', async () => {
  const { browser, page } = await attach()
  const name = `Routine locale e2e ${Date.now()}`
  let botId: string | undefined
  try {
    await setPref(page, 'appLanguage', 'it')
    await page.reload()
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', { name, backend: 'custom', command: 'unused', connectors: [], skills: [], notify: false })
    botId = bot.id
    await api(page, 'saveRoutine', {
      botId, name: 'Daily check', instruction: 'Report once', triggers: [{ type: 'cron', expression: '0 9 * * *', timeZone: 'Europe/Rome' }], timeoutSeconds: 3600,
    })
    await api(page, 'saveRoutine', { botId, name: 'Bot interval', instruction: 'Report once', triggers: [{ type: 'interval', seconds: 600 }], timeoutSeconds: 90 })
    await page.locator('.roster-row', { hasText: name }).click()
    await page.locator('.title-pill').click()

    const daily = page.locator('.routines-card', { hasText: 'Daily check' })
    await expect(daily).toContainText('Ogni giorno alle 09:00 · Europe/Rome')
    await daily.locator('.routine-row').click()

    const segmented = page.locator('.routine-form .segmented')
    await expect(segmented.getByRole('button', { name: 'Pianificazione' })).toHaveAttribute('aria-pressed', 'true')
    await expect(segmented.getByRole('button', { name: 'Webhook' })).toBeVisible()
    // The capsule sits on the chosen word, whatever its width.
    const thumbFits = () => segmented.evaluate((el) => {
      const thumb = el.querySelector<HTMLElement>('.segmented-thumb')!.getBoundingClientRect()
      const on = el.querySelector<HTMLElement>('.segmented-option.on')!.getBoundingClientRect()
      return Math.abs(thumb.left - on.left) < 1.5 && Math.abs(thumb.width - on.width) < 1.5
    })
    await expect.poll(thumbFits).toBe(true)
    await expect(page.locator('.schedule-preview')).toContainText('Ogni giorno alle 09:00 · Europe/Rome')
    await expect(page.locator('.schedule-preview')).toContainText('Prossima esecuzione:')
    await expect(page.locator('.routine-advanced')).toContainText('Impostazioni di esecuzione')
    await expect(page.locator('.routine-advanced')).toContainText('Timeout 1 ora')
    await page.locator('.routine-advanced').click()
    await expect(page.locator('.routine-form').getByText('secondi', { exact: true })).toBeVisible()

    await segmented.getByRole('button', { name: 'Webhook' }).click()
    await expect.poll(thumbFits).toBe(true)
    await page.keyboard.press('Escape')

    await page.locator('.routines-card', { hasText: 'Bot interval' }).locator('.routine-row').click()
    await expect(segmented.getByRole('button', { name: 'Impostato dal bot' })).toHaveAttribute('aria-pressed', 'true')
    await expect(page.locator('.routine-note')).toContainText('Configurato dal bot. Scegli Pianificazione o Webhook per sostituirlo.')
    await expect(page.locator('.routine-advanced')).toContainText('Timeout 90 secondi')
    await page.keyboard.press('Escape')
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await page.evaluate(() => localStorage.removeItem('appLanguage'))
    await page.reload()
    await browser.close()
  }
})
