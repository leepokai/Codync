import { test, expect } from '@playwright/test'
import { execSync } from 'node:child_process'
import type { HostSnapshot } from '../src/shared/ipc.ts'
import { attach, openGeneralSettings } from './sandbox.ts'

// Another computer's screen in the desktop app (docs/features/remote-screen.md, "Desktop viewer"). The computer the app
// runs on never offers its own. With CODYNC_E2E_SCREEN_COMPUTER naming another computer in the sandbox whose Remote
// screen is on and allows control (an SSH computer on the Lima VM `codync-linux`, its host started with DISPLAY on an
// Xvfb display), its sidebar heading's Screen button opens the viewer, which takes over from the bots and keeps Escape
// and Tab; Hand back gives Escape to the sheet again, and the computer's Settings card opens the viewer as well. A bot
// on it shows that screen live, view only, on its details' Computer tab while the window shows, and the picture opens
// the viewer, taking over while that bot isn't using the computer; a bot on this computer has the tab without a picture.
// With CODYNC_E2E_SCREEN_RESTART set to a command that restarts that computer's screen helper (`limactl shell
// codync-linux pkill -x codync-screen`: its host starts it again), Hand back and Take over each hold through the new
// session that replaces the viewer's.

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

/** Opens the first bot under `computer`'s sidebar heading and its details' Computer tab. */
async function openComputerTab(page: import('@playwright/test').Page, computer: import('@playwright/test').Locator) {
  await computer.locator('.bot-row').first().click()
  const details = page.locator('.details-panel')
  if (!(await details.isVisible())) await page.getByRole('button', { name: 'Conversation details' }).click()
  await details.getByRole('tab', { name: 'Computer', exact: true }).click()
  return details
}

test("a bot on this computer has a Computer tab without a picture", async () => {
  const { browser, page } = await attach()
  try {
    const local = await page.evaluate(
      async () => (await (window as unknown as { codync: { host: { snapshot(): Promise<HostSnapshot> } } }).codync.host.snapshot()).local,
    )
    const section = page.locator(`[data-computer-section="${local!.computerId}"]`)
    test.skip((await section.locator('.bot-row').count()) === 0, 'Needs a bot on this computer.')
    const details = await openComputerTab(page, section)
    await expect(details.locator('.computer-tab-card')).toBeVisible()
    await expect(details.locator('.computer-tab-screen')).toHaveCount(0)
    await details.getByRole('tab', { name: 'Details', exact: true }).click()
  } finally {
    await browser.close()
  }
})

test("a bot's Computer tab shows its computer's screen and opens it", async () => {
  const name = process.env.CODYNC_E2E_SCREEN_COMPUTER
  test.skip(!name, 'Needs another computer with Remote screen on: set CODYNC_E2E_SCREEN_COMPUTER (see the top of this file).')
  test.setTimeout(90_000)
  const { browser, page } = await attach()
  try {
    const section = page.locator('aside.sidebar').getByRole('region', { name })
    test.skip((await section.locator('.bot-row').count()) === 0, `Needs a bot on ${name}.`)
    const details = await openComputerTab(page, section)
    const picture = details.locator('.computer-tab-screen')
    await expect(picture.locator('video')).toBeVisible({ timeout: 30_000 })
    await picture.getByRole('button', { name: 'Open the screen' }).click()
    const viewer = page.getByRole('dialog').filter({ has: page.locator('.screen-viewer') })
    await expect(viewer.locator('video')).toBeVisible({ timeout: 30_000 })
    await expect(viewer.getByRole('button', { name: 'Hand back to bots' })).toBeVisible()
    // One stream per device: the picture lets go while the viewer has the screen.
    await expect(picture.locator('video')).toBeHidden()
    await viewer.getByRole('button', { name: 'Close' }).click()
    await expect(viewer).toHaveCount(0)
    await expect(picture.locator('video')).toBeVisible({ timeout: 30_000 })
    // A hidden window (closed to the background, minimized) lets the picture go too, and gets it back when shown.
    const showWindow = (shown: boolean) => page.evaluate((shown) => {
      if (shown) Reflect.deleteProperty(document, 'visibilityState')
      else Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => 'hidden' })
      document.dispatchEvent(new Event('visibilitychange'))
    }, shown)
    await showWindow(false)
    await expect(picture.locator('video')).toBeHidden()
    await showWindow(true)
    await expect(picture.locator('video')).toBeVisible({ timeout: 30_000 })
    await details.getByRole('tab', { name: 'Details', exact: true }).click()
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
    // Settings closed again, so the next run starts on the chat.
    await page.keyboard.press('Escape')
    await expect(page.getByRole('dialog')).toHaveCount(0)
  } finally {
    await browser.close()
  }
})

test("Hand back and Take over hold when a new session replaces the viewer's", async () => {
  const name = process.env.CODYNC_E2E_SCREEN_COMPUTER
  const restart = process.env.CODYNC_E2E_SCREEN_RESTART
  test.skip(!name || !restart, 'Needs another computer with Remote screen on and a way to restart its helper (see the top of this file).')
  test.setTimeout(150_000)
  const { browser, page } = await attach()
  try {
    const viewer = page.getByRole('dialog').filter({ has: page.locator('.screen-viewer') })
    const video = viewer.locator('video')
    /** Restarts the helper and waits for the picture to go (its session closed) and come back on a new one. */
    const replaceSession = async () => {
      await video.evaluate((element: HTMLVideoElement) => {
        (window as unknown as { screenGone: Promise<void> }).screenGone = new Promise((gone) => new MutationObserver((_, observer) => {
          if (!element.hidden) return
          observer.disconnect()
          gone()
        }).observe(element, { attributes: true, attributeFilter: ['hidden'] }))
      })
      execSync(restart!)
      await page.evaluate(() => (window as unknown as { screenGone: Promise<void> }).screenGone)
      await expect(video).toBeVisible({ timeout: 60_000 })
    }

    await page.locator('aside.sidebar').getByRole('region', { name }).getByRole('button', { name: 'Screen', exact: true }).click()
    await expect(video).toBeVisible({ timeout: 30_000 })
    await viewer.getByRole('button', { name: 'Hand back to bots' }).click()
    await replaceSession()
    await expect(viewer.getByRole('button', { name: 'Take over' })).toBeVisible()

    await viewer.getByRole('button', { name: 'Take over' }).click()
    await replaceSession()
    await expect(viewer.getByRole('button', { name: 'Hand back to bots' })).toBeVisible()
    await viewer.getByRole('button', { name: 'Close' }).click()
    await expect(viewer).toHaveCount(0)
  } finally {
    await browser.close()
  }
})
