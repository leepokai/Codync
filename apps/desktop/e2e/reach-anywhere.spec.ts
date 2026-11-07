import { test, expect, type Page } from '@playwright/test'
import type { CloudStatus } from '../src/shared/models.ts'
import { api, attach, openGeneralSettings } from './sandbox.ts'

const status = (page: Page) => api<CloudStatus>(page, 'cloudStatus', {})
const owner = async (page: Page) => (await status(page)).owner?.userId ?? null

/** As on an identity this app never saw in an account: only `keptOut` can stop `mayOfferJoin` then. */
const forgetClaims = (page: Page) =>
  page.evaluate(() => {
    for (const key of Object.keys(localStorage)) {
      if (key.startsWith('claimedComputerIds.') || key.startsWith('keptOutOfAccount.')) localStorage.removeItem(key)
    }
  })

async function openComputers(page: Page) {
  await page.reload()
  await openGeneralSettings(page)
  await page.getByRole('dialog').getByRole('button', { name: 'Computers', exact: true }).click()
}

test('turning on Reach from anywhere asks to add a computer in no account first', async () => {
  // Each relay restart keeps the old relay up for 10 seconds, and the status forgets the owner only after that.
  test.setTimeout(180_000)
  const { browser, page } = await attach()
  const started = await status(page)
  const reach = page.getByRole('switch', { name: /Reach from anywhere/ })
  const dialog = page.getByRole('alertdialog')
  const turnedOnWithoutAsking = async () => {
    await reach.click()
    await expect(reach).toHaveAttribute('aria-checked', 'true')
    await expect(dialog).toHaveCount(0)
  }
  const on = await api<CloudStatus>(page, 'setCloud', { enabled: true })
  if (!on.enabled) await browser.close()
  test.skip(!on.enabled, 'Needs the dev cloud on: `npm run dev -- --cdp --cloud`.')
  try {
    if (!on.joinsAccountOnEnable) await api(page, 'unclaim', {})
    await api(page, 'setCloud', { enabled: false })
    await forgetClaims(page)
    await openComputers(page)
    await expect(reach).toHaveAttribute('aria-checked', 'false')

    // Cancel leaves it off.
    await reach.click()
    await expect(dialog.getByRole('heading', { name: /^Add .+ to your account first$/ })).toBeVisible()
    await dialog.getByRole('button', { name: 'Cancel' }).click()
    await expect(dialog).toHaveCount(0)
    await expect(reach).toHaveAttribute('aria-checked', 'false')
    expect((await status(page)).enabled).toBe(false)

    // Use your own relay turns it on, and a computer this app never saw in the account stays out until the address is saved.
    await reach.click()
    await dialog.getByRole('button', { name: 'Use your own relay' }).click()
    await expect(dialog).toHaveCount(0)
    await expect(reach).toHaveAttribute('aria-checked', 'true')
    await expect(page.getByText('Relay address', { exact: true })).toBeVisible()
    await openComputers(page)
    await expect(page.getByText('Not in your account', { exact: true })).toBeVisible()
    await page.waitForTimeout(5_000)
    expect(await owner(page)).toBeNull()

    // A saved private relay never asks: it has no accounts, though off the status names no relay.
    await api(page, 'setCloudUrl', { url: 'https://relay.example.com' })
    await reach.click()
    await expect(dialog).toHaveCount(0)
    await expect(reach).toHaveAttribute('aria-checked', 'false')
    await expect.poll(async () => (await status(page)).relay, { timeout: 30_000 }).toBe('codync')
    await turnedOnWithoutAsking()
    expect((await status(page)).relay).toBe('private')
    await api(page, 'setCloudUrl', { url: null })

    // Add to account turns it on and adds it.
    await api(page, 'setCloud', { enabled: false })
    await expect(reach).toHaveAttribute('aria-checked', 'false')
    await reach.click()
    await dialog.getByRole('button', { name: 'Add to account' }).click()
    await expect(dialog).toHaveCount(0)
    await expect(reach).toHaveAttribute('aria-checked', 'true')
    await expect.poll(() => owner(page), { timeout: 60_000 }).not.toBeNull()

    // Off never asks, and neither does back on for a computer in an account, once the status has forgotten its owner.
    await reach.click()
    await expect(reach).toHaveAttribute('aria-checked', 'false')
    await expect(dialog).toHaveCount(0)
    await expect.poll(() => owner(page), { timeout: 30_000 }).toBeNull()
    await turnedOnWithoutAsking()
    await expect.poll(() => owner(page), { timeout: 60_000 }).not.toBeNull()
  } finally {
    // Back to how it started. A computer that started in an account ends in it, unless the run failed before adding it.
    await api(page, 'setCloudUrl', { url: null }).catch(() => {})
    if (started.joinsAccountOnEnable && (await owner(page).catch(() => null))) await api(page, 'unclaim', {}).catch(() => {})
    await api(page, 'setCloud', { enabled: started.enabled }).catch(() => {})
    await page.reload()
    await browser.close()
  }
})
