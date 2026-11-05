import { test, expect } from '@playwright/test'
import { api, attach } from './sandbox.ts'

type Usage = { providers: { id: string }[]; services: { id: string; name: string; shown: boolean }[] }

test('Settings > Usage chooses which agents show, and the host keeps the choice', async () => {
  const { browser, page } = await attach()
  const usage = () => api<Usage>(page, 'usage', {})
  const { services } = await usage()
  const service = services.find((s) => s.id === 'cursor') ?? services[0]
  test.skip(!service, 'no agent on this computer can report its usage')
  const shownAtStart = service.shown
  try {
    await api(page, 'setUsageShown', { id: service.id, shown: false })
    await page.locator('.sidebar-account').click()
    await page.locator('.account-panel').getByText('Usage', { exact: true }).click()
    const row = page.locator('.settings-usage-chooser').getByRole('switch', { name: service.name })
    await expect(row).toHaveAttribute('aria-checked', 'false')

    await row.click()
    await expect(row).toHaveAttribute('aria-checked', 'true')
    expect((await usage()).services.find((s) => s.id === service.id)?.shown).toBe(true)

    await row.click()
    await expect(row).toHaveAttribute('aria-checked', 'false')
    const after = await usage()
    expect(after.services.find((s) => s.id === service.id)?.shown).toBe(false)
    expect(after.providers.some((p) => p.id === service.id)).toBe(false)
  } finally {
    await api(page, 'setUsageShown', { id: service.id, shown: shownAtStart }).catch(() => {})
    await browser.close()
  }
})
