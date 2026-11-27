import { test, expect } from '@playwright/test'
import { resolve } from 'node:path'
import { attach } from './sandbox.ts'

const fixturePath = resolve(__dirname, 'agent-models-fixture.tsx').replace(/\\/g, '/')
const fixture = fixturePath.startsWith('/') ? `/@fs${fixturePath}` : `/@fs/${fixturePath}`

test('main and fallback model dropdowns refresh with the selected loginProfile and keep it on manual refresh', async () => {
  const { browser, page } = await attach()
  const previousLanguage = await page.evaluate(() => localStorage.getItem('appLanguage'))
  try {
    await page.evaluate(() => localStorage.setItem('appLanguage', JSON.stringify('en')))
    await page.reload()
    await page.evaluate(async (path) => {
      const { mount } = (await import(path)) as typeof import('./agent-models-fixture.tsx')
      const handle = mount()
      ;(window as unknown as { __agentModelsHandle: typeof handle }).__agentModelsHandle = handle
    }, fixture)

    const calls = () =>
      page.evaluate(() => (window as unknown as { __agentModelsHandle: { calls: () => unknown[] } }).__agentModelsHandle.calls())

    const root = page.locator('.e2e-agent-models')
    // The model panel opens over the page; its rail picks the login.
    const panel = page.locator('.model-picker')
    const list = panel.locator('.model-picker-list')
    const seat = (login: string) => panel.locator(`.model-picker-agent[aria-label="Google Antigravity · ${login}"]`)

    // Initial mount fetches default (no loginProfile) for main picker, and pro-7 for fallback #1 (order between parent/child effects is not significant).
    await expect.poll(async () => (await calls()).length).toBe(2)
    expect(await calls()).toContainEqual({ backend: 'antigravity' })
    expect(await calls()).toContainEqual({ backend: 'antigravity', loginProfile: 'pro-7' })

    // Fallback row #1 starts on Pro #7 (walked first: the bot moving onto its login would drop it, a login is in the chain once); opening it shows Pro #7's models, switching to Ultra #8 refreshes it, and Refresh keeps Ultra #8.
    const fallbackRow = root.locator('.fallback-number').locator('..')
    await fallbackRow.locator('.model-picker-field').click()
    await expect(list).toContainText('Gemini 4 Argon')
    await expect(list).not.toContainText('Gemini 4 Carbon')

    // The chain's other login (the bot's own) is no choice for it.
    await expect(seat('Default login profile')).toHaveCount(0)
    await seat('Ultra #8').click()
    await expect.poll(async () => (await calls()).slice(2)).toEqual([
      { backend: 'antigravity', loginProfile: 'ultra-8' },
    ])
    await expect(list).toContainText('Gemini 4 Carbon')
    await expect(list).not.toContainText('Gemini 4 Argon')

    await panel.getByRole('button', { name: 'Refresh model list' }).click()
    await expect.poll(async () => (await calls()).slice(3)).toEqual([
      { backend: 'antigravity', loginProfile: 'ultra-8', refresh: true },
    ])
    await page.keyboard.press('Escape')

    // Open main model dropdown -> Default tier has only Gemini 3.8 Flash.
    await root.locator('.model-picker-field').first().click()
    await expect(list).toContainText('Gemini 3.8 Flash')
    await expect(list).not.toContainText('Gemini 4 Neon')
    await expect(list).not.toContainText('Gemini 4 Carbon')

    // Switch main Login profile to Pro #7 (free now that the fallback is on Ultra #8) -> triggers agentModels with loginProfile: 'pro-7' and refreshes dropdown.
    await seat('Pro #7').click()
    await expect.poll(async () => (await calls()).slice(4)).toEqual([
      { backend: 'antigravity', loginProfile: 'pro-7' },
    ])
    await expect(list).toContainText('Gemini 4 Argon')
    await expect(list).not.toContainText('Gemini 4 Carbon')

    // Switch main Login profile to Ultra #8 -> triggers agentModels with loginProfile: 'ultra-8' and adds Gemini 4 Carbon (the fallback on it goes).
    await seat('Ultra #8').click()
    await expect.poll(async () => (await calls()).slice(5)).toEqual([
      { backend: 'antigravity', loginProfile: 'ultra-8' },
    ])
    await expect(list).toContainText('Gemini 4 Carbon')
    await expect(list).not.toContainText('Gemini 4 Argon')

    // Clicking Refresh model list keeps Ultra #8 loginProfile.
    await panel.getByRole('button', { name: 'Refresh model list' }).click()
    await expect.poll(async () => (await calls()).slice(6)).toEqual([
      { backend: 'antigravity', loginProfile: 'ultra-8', refresh: true },
    ])

    // Switch main Login profile back to Default -> omits loginProfile and reverts dropdown.
    await seat('Default login profile').click()
    await expect.poll(async () => (await calls()).slice(7)).toEqual([
      { backend: 'antigravity' },
    ])
    await expect(list).toContainText('Gemini 3.8 Flash')
    await expect(list).not.toContainText('Gemini 4 Carbon')

    // Its effort: dragging the knob from one end of the track to the other picks the last step; Space on a step picks it.
    const track = panel.getByRole('radiogroup', { name: 'Reasoning effort' })
    const step = (name: string) => track.getByRole('radio', { name })
    await expect(step('Medium')).toHaveAttribute('aria-checked', 'true')
    const box = await track.boundingBox()
    if (!box) throw new Error('the effort track has no box')
    const y = box.y + box.height / 2
    await page.mouse.move(box.x + 13, y)
    await page.mouse.down()
    await expect(step('Low')).toHaveAttribute('aria-checked', 'true')
    await page.mouse.move(box.x + box.width - 13, y, { steps: 8 })
    await page.mouse.up()
    await expect(step('High')).toHaveAttribute('aria-checked', 'true')
    await expect(panel.locator('.model-picker-effort-head')).toContainText('High')
    await step('Low').focus()
    await page.keyboard.press('Space')
    await expect(step('Low')).toHaveAttribute('aria-checked', 'true')
  } finally {
    await page
      .evaluate((previous) => {
        ;(window as unknown as { __agentModelsHandle?: { unmount: () => void } }).__agentModelsHandle?.unmount()
        if (previous === null) localStorage.removeItem('appLanguage')
        else localStorage.setItem('appLanguage', previous)
      }, previousLanguage)
      .catch(() => {})
    await page.reload().catch(() => {})
    await browser.close()
  }
})
