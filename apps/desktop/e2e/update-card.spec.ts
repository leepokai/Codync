import { test, expect } from '@playwright/test'
import { resolve } from 'node:path'
import { attach } from './sandbox.ts'

// The window loads it from the dev server, which serves files under apps/ by path.
const fixture = `/@fs${resolve(__dirname, 'update-card-fixture.tsx')}`

test("the update cards name their action in the app's language and follow a switch", async () => {
  const { browser, page } = await attach()
  try {
    await page.evaluate(() => localStorage.setItem('appLanguage', JSON.stringify('zh')))
    await page.reload()
    await page.evaluate(async (path) => {
      const { mount } = await import(path) as typeof import('./update-card-fixture.tsx')
      ;(window as unknown as { unmountCards: () => void }).unmountCards = mount()
    }, fixture)
    const cards = page.locator('.e2e-update-card')
    await expect(cards.getByText('在 Mini 上更新 Codync', { exact: true })).toBeVisible()
    await expect(cards.getByRole('button', { name: '立即更新' })).toBeVisible()
    await expect(cards.getByText('更新此应用', { exact: true })).toBeVisible()
    // Switching language in Settings redraws the button and the floor title without a reload.
    await page.evaluate(async (path) => {
      const { speak } = await import(path) as typeof import('./update-card-fixture.tsx')
      speak('en')
    }, fixture)
    await expect(cards.getByRole('button', { name: 'Update now' })).toBeVisible()
    await expect(cards.getByText('Update this app', { exact: true })).toBeVisible()
  } finally {
    await page.evaluate(() => (window as unknown as { unmountCards?: () => void }).unmountCards?.())
    await page.evaluate(() => localStorage.removeItem('appLanguage'))
    await page.reload()
    await browser.close()
  }
})
