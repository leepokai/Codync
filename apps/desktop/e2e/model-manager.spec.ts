import { test, expect } from '@playwright/test'
import { api, attach, setPref, steerBot } from './sandbox.ts'

type Hello = { backends: { id: string; name: string; available?: boolean; installed?: boolean }[] }

/** The Model Manager drives the shared picker and applies to the checked bots, on the real sandbox app. */
test('the Model Manager window opens from the header, filters, checks and applies', async () => {
  test.setTimeout(180_000)
  const { browser, page } = await attach()
  // A sandbox with no saved account covers the window with the welcome; mark the choice made, the
  // way details-sheet.spec.ts does, so the sheet under test is never behind it.
  await setPref(page, 'macAccountOnboardingCompleted', true)
  await setPref(page, 'withoutAccount', true)
  if (await page.locator('.welcome').count()) await page.reload()
  await expect(page.locator('.welcome')).toHaveCount(0, { timeout: 15_000 })
  await expect(page.locator('.sidebar')).toBeVisible({ timeout: 15_000 })
  // Previous failed runs may have left one or more sheets open over the app; close them all.
  for (let i = 0; i < 4 && (await page.locator('.model-manager').count()) > 0; i++) {
    await page.locator('.model-manager').first().locator('.model-manager-head').getByRole('button').click().catch(() => {})
    await page.waitForTimeout(400)
  }
  await expect(page.locator('.model-manager')).toHaveCount(0)

  const nameA = `Model manager e2e A ${Date.now()}`
  const nameB = `Model manager e2e B ${Date.now()}`
  const created: string[] = []
  try {
    const { backends } = await api<Hello>(page, 'hello', {})
    const agent = backends.find((b) => b.available !== false && b.installed !== false && b.id !== 'custom')
    test.skip(!agent, 'no installed agent in this sandbox')

    await test.step('create the two test bots', async () => {
      created.push((await steerBot(page, nameA, 1)).id)
      created.push((await steerBot(page, nameB, 1)).id)
    })

    await test.step('open the Model Manager from the header button', async () => {
      await page.locator('button[title="Model Manager"]').first().click()
      const sheet = page.locator('.model-manager')
      await expect(sheet).toBeVisible()
      // The top section is the shared AgentModelPicker, not a copy: same classes, two search stages.
      await expect(sheet.locator('.agent-picker')).toBeVisible()
    })

    const sheet = page.locator('.model-manager')
    const rows = sheet.locator('.model-manager-row')
    await test.step('the list carries both test bots, and the search filters it', async () => {
      await expect(rows.filter({ hasText: nameA })).toHaveCount(1)
      await expect(rows.filter({ hasText: nameB })).toHaveCount(1)
      const search = sheet.getByRole('textbox', { name: 'Search bots' })
      await search.fill(nameB)
      await expect(rows.filter({ hasText: nameA })).toHaveCount(0)
      await expect(rows.filter({ hasText: nameB })).toBeVisible()
      await search.fill('')
      await expect(rows.filter({ hasText: nameA })).toHaveCount(1)
    })

    await test.step('select all and unselect all act on the shown rows', async () => {
      await sheet.getByRole('button', { name: 'Select all', exact: true }).click()
      await expect(rows.filter({ hasText: nameA })).toHaveAttribute('aria-checked', 'true')
      await expect(rows.filter({ hasText: nameB })).toHaveAttribute('aria-checked', 'true')
      await sheet.getByRole('button', { name: 'Unselect all', exact: true }).click()
      await expect(rows.filter({ hasText: nameA })).toHaveAttribute('aria-checked', 'false')
      await expect(rows.filter({ hasText: nameB })).toHaveAttribute('aria-checked', 'false')
    })

    await test.step('pick the installed agent through the picker stages', async () => {
      const picker = sheet.locator('.agent-picker')
      // The picker starts closed: open its panel, then the agent stage, then pick.
      await picker.locator('.agent-picker-summary').click()
      await picker.locator('.agent-picker-choice').first().click()
      await picker.locator('.agent-picker-item').filter({ hasText: agent!.name }).first().click()
      await expect(picker.getByRole('textbox', { name: 'Search models' })).toBeVisible()
    })

    await test.step('check both bots and apply', async () => {
      await rows.filter({ hasText: nameA }).click()
      await rows.filter({ hasText: nameB }).click()
      await sheet.getByRole('button', { name: 'Apply' }).click()
      await expect(rows.filter({ hasText: nameA })).toContainText('Applied', { timeout: 30_000 })
      await expect(rows.filter({ hasText: nameB })).toContainText('Applied', { timeout: 30_000 })
    })

    await test.step('apply touches only the bots the filter shows (the hidden-checked pin)', async () => {
      const search = sheet.getByRole('textbox', { name: 'Search bots' })
      await sheet.getByRole('button', { name: 'Unselect all', exact: true }).click()
      await rows.filter({ hasText: nameA }).click()
      await search.fill(nameB)
      await rows.filter({ hasText: nameB }).click()
      // One checked bot (A) is hidden by the filter: the notice says Apply leaves it alone.
      await expect(sheet.getByText('hidden by the filter')).toBeVisible()
      await sheet.getByRole('button', { name: 'Apply' }).click()
      await expect(rows.filter({ hasText: nameB })).toContainText('Applied', { timeout: 30_000 })
      await search.fill('')
      // Had the bug survived, A would have been applied too (its outcome would already be set
      // by the time B's shows): the hidden bot stays untouched.
      await expect(rows.filter({ hasText: nameA })).not.toContainText('Applied')
    })

    await test.step('the saves landed: each bot editor shows the chosen agent', async () => {
      await sheet.locator('.model-manager-head').getByRole('button').click()
      await expect(sheet).toHaveCount(0)
      for (const name of [nameA, nameB]) {
        await page.locator('.roster-row', { hasText: name }).click()
        await page.getByRole('button', { name: `${name}, view details` }).click()
        await page.locator('.details-panel').getByRole('tab', { name: 'Settings' }).click()
        await expect(page.locator('.details-panel .agent-picker .agent-picker-summary')).toContainText(agent!.name)
      }
    })
  } finally {
    for (const id of created) await api(page, 'deleteBot', { botId: id }).catch(() => {})
    await browser.close()
  }
})

