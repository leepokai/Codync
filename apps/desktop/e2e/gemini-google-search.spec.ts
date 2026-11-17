import { test, expect } from '@playwright/test'
import { api, attach, openVoiceSettings, pick } from './sandbox.ts'

const prefs = ['callEngine', 'callMode.gemini', 'callGoogleSearch.gemini']

test('Gemini realtime offers a Google Search switch, off by default, kept, and only for Gemini realtime', async () => {
  const { browser, page } = await attach()
  const saved = await page.evaluate((keys) => keys.map((k) => localStorage.getItem(k)), prefs)
  try {
    // The computer says its calls can use Google Search.
    const status = await api<{ googleSearch?: boolean }>(page, 'voiceStatus', {})
    expect(status.googleSearch).toBe(true)

    await page.evaluate((keys) => {
      localStorage.setItem('callEngine', JSON.stringify('device'))
      localStorage.setItem('callMode.gemini', JSON.stringify('realtime'))
      localStorage.removeItem(keys[2])
    }, prefs)
    await page.reload()
    await openVoiceSettings(page)
    const search = page.getByRole('switch', { name: 'Google Search' })
    await expect(search).toHaveCount(0)

    await pick(page, 'On device', 'Gemini')
    await expect(search).toHaveAttribute('aria-checked', 'false')
    await search.click()
    await expect(search).toHaveAttribute('aria-checked', 'true')

    // Kept across a reload.
    await page.reload()
    await openVoiceSettings(page)
    await expect(search).toHaveAttribute('aria-checked', 'true')
    expect(await page.evaluate(() => localStorage.getItem('callGoogleSearch.gemini'))).toBe('true')

    // Speech mode has no live search; back on Realtime it is still on.
    await pick(page, 'Realtime', 'Speech')
    await expect(search).toHaveCount(0)
    await pick(page, 'Speech', 'Realtime')
    await expect(search).toHaveAttribute('aria-checked', 'true')

    // OpenAI never shows it.
    await pick(page, 'Gemini', 'OpenAI')
    await expect(search).toHaveCount(0)
  } finally {
    await page.evaluate(([keys, values]) => keys.forEach((k, i) => {
      if (values[i] === null) localStorage.removeItem(k)
      else localStorage.setItem(k, values[i]!)
    }), [prefs, saved] as const)
    await page.reload() // Closes Settings, which would cover the next spec.
    await browser.close()
  }
})
