import { test, expect, type Page } from '@playwright/test'
import { attach, openGeneralSettings } from './sandbox.ts'

// Computers → Codync Cloud → Cloud providers: the Boat key (docs/plans/codync-cloud.md, "Bring your own cloud"). Needs
// the sandbox on a cloud that offers this account Codync Cloud: `npm run cloud:stack` in cloud/, then
// `CODYNC_CLOUD_URL=<its url> npm run dev -- --cdp --cloud` (docs/guides/cloud-computer-testing.md); dev-api doesn't.
// CODYNC_E2E_BOAT_KEY, a key that cloud's Boat takes (the one in BOAT_API_KEY_FILE), adds saving, replacing and
// removing it; CODYNC_E2E_INSTABOX_KEY (the one in INSTABOX_API_KEY_FILE) does the same for InstaBox.

type Offer = { available?: boolean; cloudComputer: unknown; providers?: { provider: string; credential: unknown }[] }
type Bridge = { codync: { cloud: { request(m: string, p: string, b: unknown, s: boolean): Promise<{ ok: boolean; value?: unknown }> } } }

const cloud = (page: Page, method: string, path: string, body?: unknown) =>
  page.evaluate(([method, path, body]) => (window as unknown as Bridge).codync.cloud.request(method, path, body, false), [method, path, body] as const)

/** Starts with no keys, so the providers row asks for one and each card shows its field. */
async function forgetKeys(page: Page, offer: Offer) {
  for (const p of offer.providers ?? []) if (p.credential) await cloud(page, 'DELETE', `v1/cloud-credentials/${p.provider}`)
}

async function openComputers(page: Page) {
  await page.reload()
  await openGeneralSettings(page)
  await page.getByRole('dialog').getByRole('button', { name: 'Computers', exact: true }).click()
}

test('Codync Cloud keeps a Boat key only once Boat takes it, then offers a computer', async () => {
  const { browser, page } = await attach()
  try {
    const offer = await cloud(page, 'GET', 'v1/cloud-computers')
    const value = offer.value as Offer | undefined
    const boat = value?.providers?.find((p) => p.provider === 'boat')
    test.skip(!offer.ok || !value?.available || !boat || value.cloudComputer !== null, 'Needs a local cloud offering Codync Cloud and no cloud computer: see the top of this file.')
    await forgetKeys(page, value!)
    await openComputers(page)

    const card = page.getByRole('dialog').first()
    const create = card.getByText('Create cloud computer', { exact: true })
    // No key: nothing to create yet, and the providers row asks for one.
    await expect(card.getByText('Connect one', { exact: true })).toBeVisible()
    await expect(create).toHaveCount(0)

    // The providers sheet: each provider with its key, and how to make one folded away.
    await card.getByRole('button', { name: /Cloud providers/ }).click()
    const settings = page.getByRole('dialog').filter({ has: page.getByRole('heading', { name: 'Cloud providers' }) })
    const field = settings.getByLabel('Boat API key')
    // Boat's card is the first: its Save and its steps.
    const save = settings.getByRole('button', { name: 'Save', exact: true }).first()
    await settings.getByRole('button', { name: 'How to make a key' }).first().click()
    await expect(settings.getByText(/boat env new codync/)).toBeVisible()

    // Turned down: what to fix, under the key (in view, not below every provider), and still no key.
    await field.fill('not-a-key')
    await save.click()
    await expect(settings.getByText("That doesn't look like an API key from this provider.", { exact: true })).toBeInViewport()
    await field.fill('boat_e2e_revoked_key_0000')
    await save.click()
    await expect(settings.getByText(/^(This key wasn't accepted|The provider refused this key)/)).toBeInViewport()
    await expect(create).toHaveCount(0)

    const key = process.env.CODYNC_E2E_BOAT_KEY
    test.skip(!key, 'Set CODYNC_E2E_BOAT_KEY to a key the cloud’s Boat takes, to save, replace and remove one.')
    const hint = `${key!.slice(0, key!.indexOf('_') + 1)}…${key!.slice(-4)}`

    // Kept: only its hint shows, and a computer can be made.
    await field.fill(key!)
    await save.click()
    await expect(settings.getByText(hint, { exact: true })).toBeVisible()
    await expect(field).toHaveCount(0)
    await expect(create).toBeVisible()
    await expect(card.getByText('Boat', { exact: true })).toBeVisible()

    // A replacement Boat turns down leaves the saved key; Cancel goes back to it.
    await settings.getByRole('button', { name: 'Replace key' }).click()
    await field.fill('boat_e2e_revoked_key_0000')
    await save.click()
    await expect(settings.getByText(/^(This key wasn't accepted|The provider refused this key)/)).toBeInViewport()
    await settings.getByRole('button', { name: 'Cancel', exact: true }).click()
    await expect(settings.getByText(hint, { exact: true })).toBeVisible()

    // Removed: back to pasting one, and nothing to create.
    await settings.getByRole('button', { name: 'Remove key' }).click()
    await expect(field).toBeVisible()
    await expect(create).toHaveCount(0)
  } finally {
    await page.reload() // Closes Settings, which would cover the next spec.
    await browser.close()
  }
})

test('Codync Cloud offers InstaBox beside Boat, each with its own key', async () => {
  const { browser, page } = await attach()
  try {
    const offer = await cloud(page, 'GET', 'v1/cloud-computers')
    const value = offer.value as Offer | undefined
    const instabox = value?.providers?.find((p) => p.provider === 'instabox')
    test.skip(!offer.ok || !value?.available || !instabox || value.cloudComputer !== null, 'Needs a local cloud offering Codync Cloud and no cloud computer: see the top of this file.')
    await forgetKeys(page, value!)
    await openComputers(page)

    const card = page.getByRole('dialog').first()
    await card.getByRole('button', { name: /Cloud providers/ }).click()
    const settings = page.getByRole('dialog').filter({ has: page.getByRole('heading', { name: 'Cloud providers' }) })
    const field = settings.getByLabel('InstaBox API key')
    // InstaBox's card is the last: its Save and its steps.
    const save = settings.getByRole('button', { name: 'Save', exact: true }).last()
    await settings.getByRole('button', { name: 'How to make a key' }).last().click()
    await expect(settings.getByText(/instabox keys create --name codync --scope machines:read,machines:write,machines:delete/)).toBeVisible()

    // Refusals are the first test's; an account may save 5 keys a minute (`CLOUD_KEY_LIMITER`).
    const key = process.env.CODYNC_E2E_INSTABOX_KEY
    test.skip(!key, 'Set CODYNC_E2E_INSTABOX_KEY to a key the cloud’s InstaBox takes, to save and remove one.')
    await field.fill(key!)
    await save.click()
    await expect(settings.getByText(`ibk_…${key!.slice(-4)}`, { exact: true })).toBeVisible()
    await expect(card.getByText('Create cloud computer', { exact: true })).toBeVisible()
    await settings.getByRole('button', { name: 'Remove key' }).last().click()
    await expect(field).toBeVisible()
  } finally {
    await page.reload()
    await browser.close()
  }
})
