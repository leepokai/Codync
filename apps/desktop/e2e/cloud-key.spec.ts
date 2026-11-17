import { test, expect, type Page } from '@playwright/test'
import { attach, openGeneralSettings } from './sandbox.ts'

// Computers → Codync Cloud → "+": each provider's key, reached only through that provider (docs/plans/codync-cloud.md,
// "Bring your own cloud"). Needs the sandbox on a cloud that offers Codync Cloud: dev-api offers it to every account
// (this then removes the account's keys and sends test keys to the real providers), or, to stay off the shared dev or
// when the branch changes cloud/, `npm run cloud:stack` in cloud/ with a provider key, then
// `CODYNC_CLOUD_URL=<its url> npm run dev -- --cdp --cloud` (docs/guides/cloud-computer-testing.md).
// CODYNC_E2E_BOAT_KEY, a key that cloud's Boat takes (the one in BOAT_API_KEY_FILE), adds saving,
// replacing and removing it; CODYNC_E2E_INSTABOX_KEY (the one in INSTABOX_API_KEY_FILE) does the same for InstaBox.
// Run with `--trace off`: a trace would keep the pasted keys.

type Offer = { available?: boolean; cloudComputers?: unknown[]; providers?: { provider: string; credential: unknown }[] }
type Bridge = { codync: { cloud: { request(m: string, p: string, b: unknown, s: boolean): Promise<{ ok: boolean; value?: unknown }> } } }

const cloud = (page: Page, method: string, path: string, body?: unknown) =>
  page.evaluate(([method, path, body]) => (window as unknown as Bridge).codync.cloud.request(method, path, body, false), [method, path, body] as const)

const REFUSED = /^(This key wasn't accepted|The provider refused this key)/

/** Starts with no provider in New cloud computer having a key. */
async function forgetKeys(page: Page, offer: Offer) {
  for (const p of offer.providers ?? []) if (p.credential) await cloud(page, 'DELETE', `v1/cloud-credentials/${p.provider}`)
}

/** Computers, then "+": New cloud computer, its provider rows and the key sheets they open. */
async function openNewComputer(page: Page) {
  await page.reload()
  await openGeneralSettings(page)
  await page.getByRole('dialog').getByRole('button', { name: 'Computers', exact: true }).click()
  const card = page.locator('section.card-section').filter({ has: page.getByRole('heading', { name: 'Codync Cloud' }) })
  await expect(card).toBeVisible()
  // Keys are reached only through their provider: no row of their own.
  await expect(card.getByText('Cloud providers')).toHaveCount(0)
  await card.getByRole('button', { name: 'New cloud computer' }).click()
  const newSheet = page.getByRole('dialog').filter({ has: page.getByRole('heading', { name: 'New cloud computer' }) })
  const rowMenu = (name: string) => newSheet.locator('.cloud-provider-row', { hasText: name }).getByRole('button', { name: 'More' })
  return {
    newSheet,
    providerRow: (name: string) => newSheet.locator('button.agent-picker-item', { hasText: name }),
    rowMenu,
    /** Picks `item` in that provider's row's ⋯. */
    providerMenu: async (name: string, item: string) => {
      await rowMenu(name).click()
      await page.getByRole('menuitem', { name: item }).click()
    },
    keySheet: (name: string) => page.getByRole('dialog').filter({ has: page.getByRole('heading', { name: `${name} API key` }) }),
    create: newSheet.getByRole('button', { name: 'Create', exact: true }),
  }
}

/** Skips unless the cloud offers this account Codync Cloud and it has no cloud computer; then forgets its keys. */
async function noComputers(page: Page, provider: string) {
  const offer = await cloud(page, 'GET', 'v1/cloud-computers')
  const value = offer.value as Offer | undefined
  const listed = value?.providers?.some((p) => p.provider === provider)
  test.skip(!offer.ok || !value?.available || !listed || value.cloudComputers?.length !== 0, 'Needs a cloud offering Codync Cloud and no cloud computer: see the top of this file.')
  await forgetKeys(page, value!)
}

test('Codync Cloud keeps a Boat key only once Boat takes it, from Boat in New cloud computer', async () => {
  const { browser, page } = await attach()
  try {
    await noComputers(page, 'boat')
    const { newSheet, providerRow, providerMenu, keySheet, create } = await openNewComputer(page)
    const boatRow = providerRow('Boat')

    // No key: every provider on its logo tile, without a ⋯, asks for one (InstaBox points to its free plan),
    // and nothing can be created.
    await expect(newSheet.locator('.cloud-provider-tile .agent-icon')).toHaveCount(2)
    await expect(boatRow).toContainText('EU · Add a key first')
    await expect(providerRow('InstaBox')).toContainText('US · Free sandbox available')
    await expect(newSheet.getByRole('button', { name: 'More' })).toHaveCount(0)
    await expect(newSheet.getByLabel('Name your cloud computer')).toHaveAttribute('placeholder', 'Cloud computer')
    await expect(create).toBeDisabled()

    // Boat's row opens Boat's key alone, with its own steps and no free plan.
    await boatRow.click()
    const sheet = keySheet('Boat')
    const field = sheet.getByLabel('Boat API key')
    const save = sheet.getByRole('button', { name: 'Save', exact: true })
    await expect(sheet.getByLabel('InstaBox API key')).toHaveCount(0)
    await expect(sheet.getByRole('button', { name: 'Get a free sandbox' })).toHaveCount(0)
    await sheet.getByRole('button', { name: 'How to make a key' }).click()
    await expect(sheet.getByText(/boat env new codync/)).toBeVisible()

    // Turned down: what to fix, under the key, and still no key.
    await field.fill('not-a-key')
    await save.click()
    await expect(sheet.getByText("That doesn't look like an API key from this provider.", { exact: true })).toBeInViewport()
    await field.fill('boat_e2e_revoked_key_0000')
    await save.click()
    await expect(sheet.getByText(REFUSED)).toBeInViewport()

    const key = process.env.CODYNC_E2E_BOAT_KEY
    test.skip(!key, 'Set CODYNC_E2E_BOAT_KEY to a key the cloud’s Boat takes, to save, replace and remove one.')
    const hint = `${key!.slice(0, key!.indexOf('_') + 1)}…${key!.slice(-4)}`

    // Kept: the sheet closes, and back in New cloud computer Boat is picked with only its hint.
    await field.fill(key!)
    await save.click()
    await expect(sheet).toHaveCount(0, { timeout: 20_000 })
    await expect(boatRow).toHaveAttribute('aria-pressed', 'true')
    await expect(boatRow).toContainText(`EU · ${hint}`)
    await expect(create).toBeEnabled()

    // Its row's ⋯ reopens it: a replacement Boat turns down leaves the saved key; Cancel goes back to it.
    await providerMenu('Boat', 'Boat API key')
    await expect(sheet.getByText(hint, { exact: true })).toBeVisible()
    await sheet.getByRole('button', { name: 'Replace key' }).click()
    await field.fill('boat_e2e_revoked_key_0000')
    await save.click()
    await expect(sheet.getByText(REFUSED)).toBeInViewport()
    await sheet.getByRole('button', { name: 'Cancel', exact: true }).click()
    await expect(sheet.getByText(hint, { exact: true })).toBeVisible()
    // Closed, the refusal was seen there: New cloud computer doesn't repeat it.
    await sheet.getByRole('button', { name: 'Close' }).click()
    await expect(newSheet.getByText(REFUSED)).toHaveCount(0)

    // Removed (no computer runs on it): back to pasting one, and nothing to create on.
    await providerMenu('Boat', 'Boat API key')
    await sheet.getByRole('button', { name: 'Remove key' }).click()
    await expect(field).toBeVisible()
    await sheet.getByRole('button', { name: 'Close' }).click()
    await expect(boatRow).toContainText('Add a key first')
    await expect(create).toBeDisabled()
  } finally {
    await page.reload() // Closes Settings, which would cover the next spec.
    await browser.close()
  }
})

test('Codync Cloud offers InstaBox beside Boat, with its own key, steps and free plan', async () => {
  const { browser, page } = await attach()
  try {
    await noComputers(page, 'instabox')
    const { providerRow, rowMenu, providerMenu, keySheet, create } = await openNewComputer(page)
    const row = providerRow('InstaBox')
    await row.click()
    const sheet = keySheet('InstaBox')
    const field = sheet.getByLabel('InstaBox API key')
    await expect(sheet.getByLabel('Boat API key')).toHaveCount(0)
    await expect(sheet.getByText("InstaBox's free plan includes one machine (4 vCPU, 8 GiB).", { exact: true })).toBeVisible()
    await expect(sheet.getByRole('button', { name: 'Get a free sandbox' })).toBeVisible()
    await sheet.getByRole('button', { name: 'How to make a key' }).click()
    await expect(sheet.getByText(/instabox keys create --name codync --scope machines:read,machines:write,machines:delete/)).toBeVisible()

    // Refusals are the first test's; an account may save 5 keys a minute (`CLOUD_KEY_LIMITER`).
    const key = process.env.CODYNC_E2E_INSTABOX_KEY
    test.skip(!key, 'Set CODYNC_E2E_INSTABOX_KEY to a key the cloud’s InstaBox takes, to save and remove one.')
    await field.fill(key!)
    await sheet.getByRole('button', { name: 'Save', exact: true }).click()
    await expect(sheet).toHaveCount(0, { timeout: 20_000 })
    await expect(row).toHaveAttribute('aria-pressed', 'true')
    await expect(row).toContainText(`US · ibk_…${key!.slice(-4)}`)
    await expect(create).toBeEnabled()
    // Its row's ⋯ removes it too, while no cloud computer runs there.
    await providerMenu('InstaBox', 'Remove key')
    await expect(row).toContainText('US · Free sandbox available')
    await expect(rowMenu('InstaBox')).toHaveCount(0)
  } finally {
    await page.reload()
    await browser.close()
  }
})
