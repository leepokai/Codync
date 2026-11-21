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
    const mainPicker = root.locator('.agent-picker').first()
    const mainLogin = root.locator('.value-row', { hasText: 'Login profile' }).first().locator('.choice-picker')

    // Initial mount fetches default (no loginProfile) for main picker, and pro-7 for fallback #1 (order between parent/child effects is not significant).
    await expect.poll(async () => (await calls()).length).toBe(2)
    expect(await calls()).toContainEqual({ backend: 'antigravity' })
    expect(await calls()).toContainEqual({ backend: 'antigravity', loginProfile: 'pro-7' })

    // Open main model dropdown -> Default tier has only Gemini 3.8 Flash.
    await mainPicker.locator('.agent-picker-summary').click()
    await expect(mainPicker.locator('.agent-picker-list')).toContainText('Gemini 3.8 Flash')
    await expect(mainPicker.locator('.agent-picker-list')).not.toContainText('Gemini 4 Neon')
    await expect(mainPicker.locator('.agent-picker-list')).not.toContainText('Gemini 4 Carbon')

    // Switch main Login profile to Pro #7 -> triggers agentModels with loginProfile: 'pro-7' and refreshes dropdown.
    await mainLogin.click()
    await page.getByRole('menuitemradio', { name: 'Pro #7' }).click()
    await expect.poll(async () => (await calls()).slice(2)).toEqual([
      { backend: 'antigravity', loginProfile: 'pro-7' },
    ])
    await expect(mainPicker.locator('.agent-picker-list')).toContainText('Gemini 4 Argon')
    await expect(mainPicker.locator('.agent-picker-list')).not.toContainText('Gemini 4 Carbon')

    // Switch main Login profile to Ultra #8 -> triggers agentModels with loginProfile: 'ultra-8' and adds Gemini 4 Carbon.
    await mainLogin.click()
    await page.getByRole('menuitemradio', { name: 'Ultra #8' }).click()
    await expect.poll(async () => (await calls()).slice(3)).toEqual([
      { backend: 'antigravity', loginProfile: 'ultra-8' },
    ])
    await expect(mainPicker.locator('.agent-picker-list')).toContainText('Gemini 4 Carbon')
    await expect(mainPicker.locator('.agent-picker-list')).not.toContainText('Gemini 4 Argon')

    // Clicking Refresh model list keeps Ultra #8 loginProfile.
    await mainPicker.getByRole('button', { name: 'Refresh model list' }).click()
    await expect.poll(async () => (await calls()).slice(4)).toEqual([
      { backend: 'antigravity', loginProfile: 'ultra-8' },
    ])

    // Switch main Login profile back to Default -> omits loginProfile and reverts dropdown.
    await mainLogin.click()
    await page.getByRole('menuitemradio', { name: 'Default login profile' }).click()
    await expect.poll(async () => (await calls()).slice(5)).toEqual([
      { backend: 'antigravity' },
    ])
    await expect(mainPicker.locator('.agent-picker-list')).toContainText('Gemini 3.8 Flash')
    await expect(mainPicker.locator('.agent-picker-list')).not.toContainText('Gemini 4 Carbon')

    // Fallback row #1 starts on Pro #7; opening it shows Pro #7's models, switching to Ultra #8 refreshes it, and Refresh keeps Ultra #8.
    const fallbackRow = root.locator('.fallback-number').locator('..')
    await fallbackRow.locator('.agent-picker-summary').click()
    await expect(fallbackRow.locator('.agent-picker-list')).toContainText('Gemini 4 Argon')
    await expect(fallbackRow.locator('.agent-picker-list')).not.toContainText('Gemini 4 Carbon')

    const fallbackLogin = fallbackRow.locator('.value-row', { hasText: 'Login profile' }).locator('.choice-picker')
    await fallbackLogin.click()
    await page.getByRole('menuitemradio', { name: 'Ultra #8' }).click()
    await expect.poll(async () => (await calls()).slice(6)).toEqual([
      { backend: 'antigravity', loginProfile: 'ultra-8' },
    ])
    await expect(fallbackRow.locator('.agent-picker-list')).toContainText('Gemini 4 Carbon')
    await expect(fallbackRow.locator('.agent-picker-list')).not.toContainText('Gemini 4 Argon')

    await fallbackRow.getByRole('button', { name: 'Refresh model list' }).click()
    await expect.poll(async () => (await calls()).slice(7)).toEqual([
      { backend: 'antigravity', loginProfile: 'ultra-8' },
    ])
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
