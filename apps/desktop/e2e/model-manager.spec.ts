import { test, expect } from '@playwright/test'
import { api, attach, setPref, steerBot } from './sandbox.ts'

type Hello = { backends: { id: string; name: string; available?: boolean; installed?: boolean }[] }

/** The Bot Manager drives the shared picker and applies to the checked bots, on the real sandbox app. */
test('the Bot Manager window opens from the header, filters, checks and applies', async () => {
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

  const nameA = `Bot manager e2e A ${Date.now()}`
  const nameB = `Bot manager e2e B ${Date.now()}`
  const created: string[] = []
  try {
    const { backends } = await api<Hello>(page, 'hello', {})
    const agent = backends.find((b) => b.available !== false && b.installed !== false && b.id !== 'custom')
    test.skip(!agent, 'no installed agent in this sandbox')

    await test.step('create the two test bots', async () => {
      created.push((await steerBot(page, nameA, 1)).id)
      created.push((await steerBot(page, nameB, 1)).id)
    })

    await test.step('open the Bot Manager from the header button', async () => {
      await page.locator('button[title="Bot Manager"]').first().click()
      const sheet = page.locator('.model-manager')
      await expect(sheet).toBeVisible()
      // The top section is the shared ModelPicker, not a copy: same field, seat rail and model list.
      await expect(sheet.locator('.model-picker-field')).toBeVisible()
    })

    const sheet = page.locator('.model-manager')
    const rows = sheet.locator('.model-manager-row')
    await test.step('the list carries both test bots, and the search filters it', async () => {
      await expect(rows.filter({ hasText: nameA })).toHaveCount(1)
      await expect(rows.filter({ hasText: nameB })).toHaveCount(1)
      // The row's small agent->model line stays honest: a custom-command bot shows none
      // (rowModelPath's null contract; the readable-model rendering is unit-pinned).
      await expect(rows.filter({ hasText: nameA }).locator('.model-manager-model-path')).toHaveCount(0)
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

    await test.step('pick the installed agent through the picker', async () => {
      // The shared picker opens on its models with the agents as its seat rail; one click picks.
      // The seat's accessible name is the agent's own name (the rail shows no other text).
      await sheet.locator('.model-picker-field').click()
      await page.locator('.model-picker-rail').getByRole('button', { name: agent!.name, exact: false }).click()
      await expect(page.getByRole('textbox', { name: 'Search models' })).toBeVisible()
      // The picker overlays the sheet until dismissed. The search box holds focus, so a user
      // closes it by clicking the backdrop (the menu's own outside-click close), not Escape.
      await page.mouse.click(10, 10)
      await expect(page.locator('.model-picker')).toHaveCount(0)
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
      await expect(rows.filter({ hasText: nameA })).not.toContainText('Applied', { timeout: 30_000 })
    })

    await test.step('pause checked pauses both bots, and resume checked resumes them', async () => {
      await sheet.getByRole('button', { name: 'Unselect all', exact: true }).click()
      await rows.filter({ hasText: nameA }).click()
      await rows.filter({ hasText: nameB }).click()
      await sheet.getByRole('button', { name: 'Pause checked', exact: true }).click()
      await expect(rows.filter({ hasText: nameA })).toContainText('Paused', { timeout: 30_000 })
      await expect(rows.filter({ hasText: nameB })).toContainText('Paused', { timeout: 30_000 })
      await sheet.getByRole('button', { name: 'Resume checked', exact: true }).click()
      await expect(rows.filter({ hasText: nameA })).toContainText('Resumed', { timeout: 30_000 })
      await expect(rows.filter({ hasText: nameB })).toContainText('Resumed', { timeout: 30_000 })
      // The pause is really gone: the host echoed the resume, so no row still says Paused. The
      // roster echo can trail the local outcome, so the negatives wait as long as the positives.
      await expect(rows.filter({ hasText: nameA })).not.toContainText('Paused', { timeout: 30_000 })
      await expect(rows.filter({ hasText: nameB })).not.toContainText('Paused', { timeout: 30_000 })
    })

    await test.step('a hidden-checked bot is not paused either (the filtered-pause pin)', async () => {
      await sheet.getByRole('button', { name: 'Unselect all', exact: true }).click()
      await rows.filter({ hasText: nameA }).click()
      const search = sheet.getByRole('textbox', { name: 'Search bots' })
      await search.fill(nameB)
      await rows.filter({ hasText: nameB }).click()
      // One checked bot (A) is hidden by the filter: Pause checked must leave it alone, exactly
      // as Apply does — the wiring is the same targets.chosen, and this step holds it there.
      await sheet.getByRole('button', { name: 'Pause checked', exact: true }).click()
      await expect(rows.filter({ hasText: nameB })).toContainText('Paused', { timeout: 30_000 })
      await search.fill('')
      await expect(rows.filter({ hasText: nameA })).not.toContainText('Paused', { timeout: 30_000 })
      // Clean up for whatever comes after: resume B, clear the outcome rows.
      await sheet.getByRole('button', { name: 'Unselect all', exact: true }).click()
      await rows.filter({ hasText: nameB }).click()
      await sheet.getByRole('button', { name: 'Resume checked', exact: true }).click()
      await expect(rows.filter({ hasText: nameB })).toContainText('Resumed', { timeout: 30_000 })
      await expect(rows.filter({ hasText: nameB })).not.toContainText('Paused', { timeout: 30_000 })
    })

    await test.step('the saves landed: each bot editor shows the chosen agent', async () => {
      await sheet.locator('.model-manager-head').getByRole('button').click()
      await expect(sheet).toHaveCount(0)
      for (const name of [nameA, nameB]) {
        await page.locator('.roster-row', { hasText: name }).click()
        await page.getByRole('button', { name: `${name}, view details` }).click()
        const panel = page.locator('.details-panel')
        // The panel keeps its layout: an earlier run's panel shows its tabs, a fresh one the chooser.
        // Both are real states of the same window, so the step takes whichever Settings it finds.
        const settingsTab = panel.getByRole('tab', { name: 'Settings' })
        if ((await settingsTab.count()) === 0) await panel.locator('.panel-choice', { hasText: 'Settings' }).click()
        else await settingsTab.click()
        await expect(page.locator('.details-panel .model-picker-field')).toContainText(agent!.name)
      }
    })
  } finally {
    for (const id of created) await api(page, 'deleteBot', { botId: id }).catch(() => {})
    await browser.close()
  }
})

