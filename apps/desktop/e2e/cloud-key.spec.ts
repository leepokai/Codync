import { test, expect, type Page } from '@playwright/test'
import { attach, openGeneralSettings } from './sandbox.ts'

// Computers → Codync Cloud's Boat key (docs/plans/codync-cloud.md, "Bring your own cloud"). Needs the sandbox on a
// cloud that offers this account Codync Cloud: `npm run cloud:stack` in cloud/, then
// `CODYNC_CLOUD_URL=<its url> npm run dev -- --cdp --cloud` (docs/guides/cloud-computer-testing.md); dev-api doesn't.
// CODYNC_E2E_BOAT_KEY, a key that cloud's Boat takes (the one in BOAT_API_KEY_FILE), adds saving, replacing and
// removing it.

type Offer = { available?: boolean; cloudComputer: unknown; keySetup?: { commands: string } | null; credential?: unknown }
type Bridge = { codync: { cloud: { request(m: string, p: string, b: unknown, s: boolean): Promise<{ ok: boolean; value?: unknown }> } } }

const cloud = (page: Page, method: string, path: string, body?: unknown) =>
  page.evaluate(([method, path, body]) => (window as unknown as Bridge).codync.cloud.request(method, path, body, false), [method, path, body] as const)

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
    test.skip(!offer.ok || !value?.available || !value.keySetup || value.cloudComputer !== null, 'Needs a local cloud offering Codync Cloud and no cloud computer: see the top of this file.')
    if (value?.credential) await cloud(page, 'DELETE', 'v1/cloud-credentials/boat')
    await openComputers(page)

    const settings = page.getByRole('dialog')
    const field = settings.getByLabel('Boat API key')
    const save = settings.getByRole('button', { name: 'Save', exact: true })
    const create = settings.getByText('Create cloud computer', { exact: true })

    // No key: how to make one, a field to paste it, and nothing to create yet.
    await expect(settings.getByText(/boat env new codync/)).toBeVisible()
    await expect(create).toHaveCount(0)

    // Turned down: what to fix, and still no key.
    await field.fill('not-a-key')
    await save.click()
    await expect(settings.getByText("That doesn't look like a Boat API key.", { exact: true })).toBeVisible()
    await field.fill('boat_e2e_revoked_key_0000')
    await save.click()
    await expect(settings.getByText(/^Boat (didn't accept|refused) this key/)).toBeVisible()
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

    // A replacement Boat turns down leaves the saved key; Cancel goes back to it.
    await settings.getByRole('button', { name: 'Replace key' }).click()
    await field.fill('boat_e2e_revoked_key_0000')
    await save.click()
    await expect(settings.getByText(/^Boat (didn't accept|refused) this key/)).toBeVisible()
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
