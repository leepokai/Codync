import { test, expect } from '@playwright/test'
import type { HostSnapshot } from '../src/shared/ipc.ts'
import { attach, openGeneralSettings } from './sandbox.ts'

// Another computer's screen in the desktop app (docs/features/remote-screen.md, "Desktop viewer"). The computer the app
// runs on never offers its own. With CODYNC_E2E_SCREEN_COMPUTER naming another computer in the sandbox whose Remote
// screen is on and allows control (an SSH computer on the Lima VM `codync-linux`, its host started with DISPLAY on an
// Xvfb display), its sidebar heading's Screen button opens the viewer, which takes over from the bots and keeps Escape
// and Tab; Hand back gives Escape to the sheet again, and the computer's Settings card opens the viewer as well.

test('this computer offers no Screen button for itself', async () => {
  const { browser, page } = await attach()
  try {
    const local = await page.evaluate(
      async () => (await (window as unknown as { codync: { host: { snapshot(): Promise<HostSnapshot> } } }).codync.host.snapshot()).local,
    )
    expect(local).toBeTruthy()
    await expect(page.locator(`[data-computer-section="${local!.computerId}"]`).getByRole('button', { name: 'Screen', exact: true })).toHaveCount(0)
  } finally {
    await browser.close()
  }
})

test("another computer's screen takes the controls, keeps Escape and Tab, and hands them back", async () => {
  const name = process.env.CODYNC_E2E_SCREEN_COMPUTER
  test.skip(!name, 'Needs another computer with Remote screen on: set CODYNC_E2E_SCREEN_COMPUTER (see the top of this file).')
  test.setTimeout(90_000)
  const { browser, page } = await attach()
  try {
    const viewer = page.getByRole('dialog').filter({ has: page.locator('.screen-viewer') })
    const keyboard = viewer.locator('textarea[data-captures-keys]')
    /** Live and in control: the picture plays and the keyboard field takes every key. */
    const controlling = async () => {
      await expect(viewer.locator('video')).toBeVisible({ timeout: 30_000 })
      await expect(keyboard).toBeFocused({ timeout: 30_000 })
      await expect(viewer.getByRole('button', { name: 'Hand back to bots' })).toBeVisible()
    }

    await page.locator('aside.sidebar').getByRole('region', { name }).getByRole('button', { name: 'Screen', exact: true }).click()
    await controlling()
    await page.keyboard.press('Escape')
    await page.keyboard.press('Tab')
    await expect(viewer).toBeVisible()
    await expect(keyboard).toBeFocused()

    await viewer.getByRole('button', { name: 'Hand back to bots' }).click()
    await expect(viewer.getByRole('button', { name: 'Take over' })).toBeVisible()
    await expect(keyboard).toHaveCount(0)
    await page.keyboard.press('Escape')
    await expect(viewer).toHaveCount(0)

    await openGeneralSettings(page)
    await page.getByRole('dialog').getByRole('button', { name: 'Computers', exact: true }).click()
    const card = page.getByRole('dialog').locator('.settings-row').filter({ has: page.getByText(name!, { exact: true }) })
    await card.getByRole('button', { name: 'Screen', exact: true }).click()
    await controlling()
    await viewer.getByRole('button', { name: 'Close' }).click()
    await expect(viewer).toHaveCount(0)
  } finally {
    await browser.close()
  }
})
