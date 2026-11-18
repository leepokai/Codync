import { test, expect } from '@playwright/test'
import { resolve } from 'node:path'
import { attach } from './sandbox.ts'

// The window loads it from the dev server, which serves files under apps/ by path.
const fixture = `/@fs${resolve(__dirname, 'host-failed-fixture.tsx')}`
// A host error that ends like "{count} installed" and must not become "已安装 … 个".
const error = "Error: the host service isn't installed"

test('a failed host shows its sad face, its error untouched in Chinese, and Try again', async () => {
  const { browser, page } = await attach()
  try {
    await page.evaluate(() => localStorage.setItem('appLanguage', JSON.stringify('zh')))
    await page.reload()
    await page.evaluate(async ([path, message]) => {
      const { mount } = await import(path) as typeof import('./host-failed-fixture.tsx')
      ;(window as unknown as { unmountFailedHost: () => void }).unmountFailedHost = mount(message)
    }, [fixture, error] as const)
    const screen = page.locator('.e2e-host-failed .empty-state')
    await expect(screen.getByText('Host 没有在运行', { exact: true })).toBeVisible()
    await expect(screen.getByText(error, { exact: true })).toBeVisible()
    await expect(screen.getByText(/已安装/)).toHaveCount(0)
    await expect(screen.locator('.host-state-bot.is-failed canvas')).toBeVisible()
    await screen.locator('.host-state-bot canvas').screenshot({ path: test.info().outputPath('failed-face.png') })
    await screen.getByRole('button', { name: '再试一次' }).click()
  } finally {
    await page.evaluate(() => (window as unknown as { unmountFailedHost?: () => void }).unmountFailedHost?.())
    await page.evaluate(() => localStorage.removeItem('appLanguage'))
    await page.reload()
    await browser.close()
  }
})
