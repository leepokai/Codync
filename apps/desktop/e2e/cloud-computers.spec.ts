import { test, expect, type Page } from '@playwright/test'
import { attach, openGeneralSettings } from './sandbox.ts'

// Computers → Codync Cloud with real machines: "+" → New cloud computer lists every provider with its logo; one without
// a key opens only its own key sheet and comes back picked once the key is saved; a named computer on InstaBox and one on
// Boat claim themselves under their names; a computer's `⋯` opens its provider's key (kept while the computer exists),
// *Rename…* reaches the card, the sidebar and the account; a refused replacement keeps the saved key; a third computer
// fills the account, where Create says why it's off; all three are deleted after asking by name; then the keys go
// (docs/guides/cloud-computer-testing.md, "Real apps against a local cloud").
// It makes real machines (minutes, and the providers' time), so it runs only with CODYNC_E2E_CLOUD_MACHINES=1 beside
// CODYNC_E2E_BOAT_KEY and CODYNC_E2E_INSTABOX_KEY, on the sandbox pointed at `npm run cloud:stack` with
// CODYNC_HOST_LINUX_BIN (cloud-key.spec.ts says how). Run with `--trace off`: a trace would keep the pasted keys.

type Machine = { cloudComputerId: string; name: string; provider: string; computerId: string | null }
type Offer = { available?: boolean; cloudComputers?: Machine[]; providers?: { provider: string; credential: unknown }[] }
type Bridge = { codync: { cloud: { request(m: string, p: string, b: unknown, s: boolean): Promise<{ ok: boolean; value?: unknown }> } } }

const cloud = (page: Page, method: string, path: string, body?: unknown) =>
  page.evaluate(([method, path, body]) => (window as unknown as Bridge).codync.cloud.request(method, path, body, false), [method, path, body] as const)

const accountNames = async (page: Page) =>
  (((await cloud(page, 'GET', 'v1/computers')).value as { computers?: { name: string }[] } | undefined)?.computers ?? []).map((c) => c.name)

test('Codync Cloud runs named cloud computers on Boat and InstaBox side by side', async () => {
  const boatKey = process.env.CODYNC_E2E_BOAT_KEY
  const instaboxKey = process.env.CODYNC_E2E_INSTABOX_KEY
  test.skip(!process.env.CODYNC_E2E_CLOUD_MACHINES || !boatKey || !instaboxKey, 'Makes real machines: set CODYNC_E2E_CLOUD_MACHINES=1 and both keys (see the top of this file).')
  test.setTimeout(30 * 60_000)
  const { browser, page } = await attach()
  try {
    const value = (await cloud(page, 'GET', 'v1/cloud-computers')).value as Offer | undefined
    test.skip(!value?.available || value.cloudComputers?.length !== 0, 'Needs a local cloud offering Codync Cloud and no cloud computer.')
    for (const p of value!.providers ?? []) if (p.credential) await cloud(page, 'DELETE', `v1/cloud-credentials/${p.provider}`)
    await page.reload()
    await openGeneralSettings(page)
    await page.getByRole('dialog').getByRole('button', { name: 'Computers', exact: true }).click()

    const section = page.locator('section.card-section').filter({ has: page.getByRole('heading', { name: 'Codync Cloud' }) })
    const newSheet = page.getByRole('dialog').filter({ has: page.getByRole('heading', { name: 'New cloud computer' }) })
    const keySheet = (name: string) => page.getByRole('dialog').filter({ has: page.getByRole('heading', { name: `${name} API key` }) })
    const providerRow = (name: string) => newSheet.locator('button.agent-picker-item', { hasText: name })
    const rowMenu = (name: string) => newSheet.locator('.cloud-provider-row', { hasText: name }).getByRole('button', { name: 'More' })
    const nameField = newSheet.getByLabel('Name your cloud computer')
    const create = newSheet.getByRole('button', { name: 'Create', exact: true })
    const row = (name: string) => section.locator('.settings-row').filter({ has: page.getByText(name, { exact: true }) })
    const sidebar = (name: string) => page.locator('aside.sidebar').getByRole('region', { name })
    const openMenu = (name: string) => row(name).getByRole('button', { name: 'More' }).click()
    const menu = async (name: string, item: string) => {
      await openMenu(name)
      await page.getByRole('menuitem', { name: item }).click()
    }
    const hint = (key: string) => `${key.slice(0, key.indexOf('_') + 1)}…${key.slice(-4)}`
    /** Saves `key` in the open key sheet, which closes once the cloud keeps it. */
    const saveKey = async (name: string, key: string) => {
      await keySheet(name).getByLabel(`${name} API key`).fill(key)
      await keySheet(name).getByRole('button', { name: 'Save', exact: true }).click()
      await expect(keySheet(name)).toHaveCount(0, { timeout: 20_000 })
    }
    /** Created from the open New cloud computer sheet: it closes, and the row prepares, then joins the account. */
    const createAndClaim = async (name: string, line: string) => {
      await create.click()
      await expect(newSheet).toHaveCount(0, { timeout: 60_000 })
      await expect(row(name)).toContainText(`${line} · Preparing your cloud computer…`)
      await expect(row(name)).not.toContainText('Preparing', { timeout: 15 * 60_000 })
      await expect.poll(() => accountNames(page), { timeout: 60_000 }).toEqual(expect.arrayContaining([name]))
      await expect(sidebar(name)).toBeVisible({ timeout: 120_000 })
    }

    // No Cloud providers row: "+" lists every provider on its logo tile, and InstaBox without a key opens only its key.
    await expect(section).toBeVisible()
    await expect(section.getByText('Cloud providers')).toHaveCount(0)
    await section.getByRole('button', { name: 'New cloud computer' }).click()
    await expect(newSheet.locator('.cloud-provider-tile .agent-icon')).toHaveCount(2)
    await expect(providerRow('Boat')).toContainText('Add a key first')
    await expect(nameField).toHaveAttribute('placeholder', 'Cloud computer')
    await expect(create).toBeDisabled()
    await providerRow('InstaBox').click()
    await expect(keySheet('InstaBox').getByLabel('Boat API key')).toHaveCount(0)
    await saveKey('InstaBox', instaboxKey!)

    // Back with InstaBox picked: named Research, it claims itself under that name.
    await expect(providerRow('InstaBox')).toHaveAttribute('aria-pressed', 'true')
    await expect(providerRow('InstaBox')).toContainText(hint(instaboxKey!))
    await nameField.fill('Research')
    await createAndClaim('Research', 'InstaBox · US')
    expect(await accountNames(page)).not.toContain('Codync Cloud')
    // InstaBox keeps no snapshots: its menu has nothing of them to erase.
    await openMenu('Research')
    await expect(page.getByRole('menuitem')).toHaveText(['Rename…', 'InstaBox API key', 'Delete cloud computer'])
    await page.keyboard.press('Escape')

    // Its `⋯` opens InstaBox's key alone: the hint and Replace, and no Remove while Research runs on it.
    await menu('Research', 'InstaBox API key')
    await expect(keySheet('InstaBox').getByText(hint(instaboxKey!), { exact: true })).toBeVisible()
    await expect(keySheet('InstaBox').getByRole('button', { name: 'Replace key' })).toBeVisible()
    await expect(keySheet('InstaBox').getByRole('button', { name: 'Remove key' })).toHaveCount(0)
    await keySheet('InstaBox').getByRole('button', { name: 'Close' }).click()

    // A Boat key from Boat's row, and a computer there with the suggested name beside Research, both with their logos.
    await section.getByRole('button', { name: 'New cloud computer' }).click()
    await providerRow('Boat').click()
    await saveKey('Boat', boatKey!)
    await expect(providerRow('Boat')).toHaveAttribute('aria-pressed', 'true')
    await expect(nameField).toHaveAttribute('placeholder', 'Cloud computer')
    await create.click()
    await expect(newSheet).toHaveCount(0, { timeout: 60_000 })
    await expect(row('Cloud computer')).toContainText('Boat · EU · ')
    await expect(section.locator('.settings-row .agent-icon')).toHaveCount(2)
    // Boat keeps snapshots, but none while it prepares.
    await openMenu('Cloud computer')
    await expect(page.getByRole('menuitem', { name: 'Delete snapshots' })).toBeDisabled()
    await page.keyboard.press('Escape')
    await expect(row('Cloud computer')).not.toContainText('Preparing', { timeout: 10 * 60_000 })
    await expect(sidebar('Cloud computer')).toBeVisible({ timeout: 120_000 })

    // Renamed: the card, the account's computers and the sidebar follow.
    await menu('Research', 'Rename…')
    const naming = page.getByRole('alertdialog').filter({ hasText: 'Rename cloud computer' })
    await naming.getByLabel('Name').fill('Lab 2')
    await naming.getByRole('button', { name: 'Save' }).click()
    await expect(row('Lab 2')).toBeVisible()
    await expect(row('Research')).toHaveCount(0)
    await expect.poll(() => accountNames(page), { timeout: 30_000 }).toEqual(expect.arrayContaining(['Lab 2']))
    await expect(sidebar('Lab 2')).toBeVisible({ timeout: 60_000 })
    await expect(sidebar('Research')).toHaveCount(0)

    // "+": both keys usable, each with its row's ⋯, and the next free name.
    await section.getByRole('button', { name: 'New cloud computer' }).click()
    await expect(nameField).toHaveAttribute('placeholder', 'Cloud computer 2')
    await expect(rowMenu('InstaBox')).toBeVisible()
    // Boat's ⋯ has no Remove key while a computer runs there, and opens Boat's key alone; a replacement Boat turns down
    // shows what to fix there and keeps the saved key, and once it's closed New cloud computer doesn't repeat it.
    await rowMenu('Boat').click()
    await expect(page.getByRole('menuitem')).toHaveText(['Boat API key'])
    await page.getByRole('menuitem', { name: 'Boat API key' }).click()
    const boat = keySheet('Boat')
    await expect(boat.getByLabel('InstaBox API key')).toHaveCount(0)
    await boat.getByRole('button', { name: 'Replace key' }).click()
    await boat.getByLabel('Boat API key').fill('boat_e2e_revoked_key_0000')
    await boat.getByRole('button', { name: 'Save', exact: true }).click()
    await expect(boat.getByText(/^(This key wasn't accepted|The provider refused this key)/)).toBeInViewport({ timeout: 20_000 })
    await boat.getByRole('button', { name: 'Cancel', exact: true }).click()
    await expect(boat.getByText(hint(boatKey!), { exact: true })).toBeVisible()
    await boat.getByRole('button', { name: 'Close' }).click()
    await expect(newSheet.getByText(/^(This key wasn't accepted|The provider refused this key)/)).toHaveCount(0)

    // A third, named Third on InstaBox: then the account is full, and Create says why while the keys stay in reach.
    await providerRow('InstaBox').click()
    await nameField.fill('Third')
    await createAndClaim('Third', 'InstaBox · US')
    await section.getByRole('button', { name: 'New cloud computer' }).click()
    await expect(newSheet.getByText('You can have up to 3 cloud computers. Delete one to make another.', { exact: true })).toBeVisible()
    await expect(create).toBeDisabled()
    await expect(rowMenu('Boat')).toBeEnabled()
    await newSheet.getByRole('button', { name: 'Cancel', exact: true }).click()

    // Deleted after asking by name: gone from the card, the sidebar and the account.
    for (const name of ['Lab 2', 'Cloud computer', 'Third']) {
      await menu(name, 'Delete cloud computer')
      await page.getByRole('alertdialog').filter({ hasText: `Delete ${name}?` }).getByRole('button', { name: 'Delete' }).click()
      await expect(row(name)).toHaveCount(0, { timeout: 60_000 })
      await expect(sidebar(name)).toHaveCount(0, { timeout: 60_000 })
    }
    await expect.poll(() => accountNames(page), { timeout: 30_000 }).toEqual([])
    expect(((await cloud(page, 'GET', 'v1/cloud-computers')).value as Offer).cloudComputers).toEqual([])

    // With no computer left, each key goes from its row's ⋯ in New cloud computer.
    await section.getByRole('button', { name: 'New cloud computer' }).click()
    for (const [name, keyless] of [['Boat', 'Add a key first'], ['InstaBox', 'Free sandbox available']]) {
      await rowMenu(name).click()
      await page.getByRole('menuitem', { name: 'Remove key' }).click()
      await expect(providerRow(name)).toContainText(keyless)
    }
    await expect(create).toBeDisabled()
  } finally {
    // Whatever a failure left: the machines, then the keys.
    const left = ((await cloud(page, 'GET', 'v1/cloud-computers')).value as Offer | undefined)?.cloudComputers ?? []
    for (const m of left) await cloud(page, 'DELETE', `v1/cloud-computers/${m.cloudComputerId}`)
    for (const provider of ['boat', 'instabox']) await cloud(page, 'DELETE', `v1/cloud-credentials/${provider}`)
    await page.reload()
    await browser.close()
  }
})
