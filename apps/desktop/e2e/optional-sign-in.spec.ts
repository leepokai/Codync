import { test, expect, type Page } from '@playwright/test'
import type { CloudStatus } from '../src/shared/models.ts'
import { pairAsSignedOutPhone } from './phone.ts'
import { api, attach, openGeneralSettings, sandbox, setPref } from './sandbox.ts'

// docs/plans/optional-sign-in.md.

const status = (page: Page) => api<CloudStatus>(page, 'cloudStatus', {})
const owner = async (page: Page) => (await status(page)).owner?.userId ?? null

/** As on an identity this app never saw or kept out of an account. */
const forgetChoices = (page: Page) =>
  page.evaluate(() => {
    for (const key of Object.keys(localStorage)) {
      if (key.startsWith('claimedComputerIds.') || key.startsWith('keptOutOfAccount.')) localStorage.removeItem(key)
    }
  })

// Two pairing attempts: a second run within a minute goes over the host's three a minute, as it should.
test('a phone with no account is asked about, on a computer in no account', async () => {
  const { browser, page } = await attach()
  const dialog = page.getByRole('alertdialog')
  const devices = async () => (await api<{ devices: { name: string; key: string }[] }>(page, 'devices', {})).devices
  const pair = async (name: string) => {
    const { pairingUrl } = await api<{ pairingUrl: string }>(page, 'pairing', {})
    const { computerId } = await api<{ computerId: string }>(page, 'hello', {})
    const code = new URL(pairingUrl).searchParams.get('code') as string
    return pairAsSignedOutPhone(sandbox().port, computerId, code, name)
  }
  try {
    test.skip((await owner(page)) !== null, 'Needs a computer in no account: `npm run dev -- --cdp`, without `--cloud`.')

    const refused = pair('Stranger phone')
    await expect(dialog.getByRole('heading', { name: 'Let Stranger phone in?' })).toBeVisible()
    await expect(dialog.getByText(/It can connect only over Wi-Fi or Tailscale/)).toBeVisible()
    await expect(dialog.getByRole('button', { name: 'Add to account' })).toHaveCount(0)
    await dialog.getByRole('button', { name: "Don't allow" }).click()
    expect((await refused).err?.status).toBe(403)

    const allowed = pair('E2E phone')
    await expect(dialog.getByRole('heading', { name: 'Let E2E phone in?' })).toBeVisible()
    await dialog.getByRole('button', { name: 'Let in' }).click()
    expect((await allowed).ok).toBeTruthy()
    const phone = (await devices()).find((device) => device.name === 'E2E phone')
    expect(phone).toBeTruthy()
    await api(page, 'revokeDevice', { key: phone?.key })
  } finally {
    await browser.close()
  }
})

test('the welcome lets you go on without an account, and it stays that way', async () => {
  const { browser, page } = await attach()
  const goOn = page.getByRole('button', { name: /^Continue without an account/ })
  const restore = async () => {
    await setPref(page, 'macAccountOnboardingCompleted', true)
    await setPref(page, 'withoutAccount', false)
    await page.reload()
  }
  try {
    await setPref(page, 'macAccountOnboardingCompleted', false)
    await page.reload()
    // Signed in, the welcome finishes on its own.
    test.skip(!(await goOn.isVisible({ timeout: 5_000 }).catch(() => false)), 'Needs the app signed out in a build with accounts.')
    await goOn.click()
    await expect(goOn).toHaveCount(0)
    // Computer access goes back to the way of signing in.
    await setPref(page, 'computerAccessSetup', null)
    await page.getByRole('button', { name: 'Back' }).click()
    await expect(goOn).toBeVisible()
    await goOn.click()
    await expect(goOn).toHaveCount(0)
    await setPref(page, 'computerAccessSetup', 'later')
    await page.reload()
    await expect(page.locator('#root')).not.toBeEmpty()
    await expect(goOn).toHaveCount(0)
  } finally {
    await restore()
    await browser.close()
  }
})

test('signed in, a computer in no account is asked about once, and Add to account asks again', async () => {
  // Leaving an account and joining one each wait on the dev cloud.
  test.setTimeout(180_000)
  const { browser, page } = await attach()
  const prompt = page.getByRole('alertdialog')
  const question = prompt.getByRole('heading', { name: 'Add this computer to your account?' })
  const started = await status(page)
  const on = await api<CloudStatus>(page, 'setCloud', { enabled: true })
  if (!on.enabled) await browser.close()
  test.skip(!on.enabled, 'Needs the dev cloud on and a signed-in sandbox: `npm run dev -- --cdp --cloud`.')
  try {
    if (on.owner) await api(page, 'unclaim', {})
    await expect.poll(() => owner(page), { timeout: 30_000 }).toBeNull()
    await forgetChoices(page)
    await page.reload()

    await expect(question).toBeVisible({ timeout: 20_000 })
    await prompt.getByRole('button', { name: 'Not now' }).click()
    await expect(prompt).toHaveCount(0)
    expect(await owner(page)).toBeNull()
    await page.reload()
    await page.waitForTimeout(3_000)
    await expect(prompt).toHaveCount(0)

    await openGeneralSettings(page)
    await page.getByRole('dialog').getByRole('button', { name: 'Computers', exact: true }).click()
    await expect(page.getByText('Not in an account: only Wi-Fi and Tailscale reach it.')).toBeVisible()

    await page.getByRole('button', { name: 'Add to account' }).click()
    await expect(question).toBeVisible()
    await prompt.getByRole('button', { name: 'Add to account' }).click()
    await expect(prompt).toHaveCount(0)
    await expect.poll(() => owner(page), { timeout: 60_000 }).not.toBeNull()
  } finally {
    if (!started.owner && (await owner(page).catch(() => null))) await api(page, 'unclaim', {}).catch(() => {})
    await api(page, 'setCloud', { enabled: started.enabled }).catch(() => {})
    await forgetChoices(page).catch(() => {})
    await page.reload()
    await browser.close()
  }
})
