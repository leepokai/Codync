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
  // Previous failed runs may have left one or more sheets open over the app, and maybe an anchored
  // panel over them; dismiss the panel first, then close the sheets.
  for (let i = 0; i < 4 && (await page.locator('.model-manager').count()) > 0; i++) {
    await page.keyboard.press('Escape').catch(() => {})
    await page.locator('.model-manager').first().locator('.model-manager-head').getByRole('button').click({ timeout: 4000 }).catch(() => {})
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
      // The shared search field's clear ✕: it shows only while there is text, empties the
      // field, and hands focus back so the next filter is ready to type.
      const clear = sheet.locator('.model-manager-body .search-clear')
      await expect(clear).toBeVisible()
      await clear.click()
      await expect(search).toHaveValue('')
      await expect(search).toBeFocused()
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

    await test.step('pick the installed agent through the shared picker', async () => {
      // The picker starts closed: its row opens the shared ModelPicker panel (portaled to the root,
      // like every anchored menu); the seat rail picks the agent.
      await sheet.locator('.model-picker-field').click()
      const panel = page.locator('.anchored-panel.model-picker')
      await expect(panel).toBeVisible()
      await panel.locator(`.model-picker-agent[title^="${agent!.name}"]`).first().click()
      await expect(panel.getByRole('textbox', { name: 'Search models' })).toBeVisible()
      // Close the panel with Escape (the anchored menu's own key) so the rows beneath are bare.
      await page.keyboard.press('Escape')
      await expect(panel).toHaveCount(0)
    })

    await test.step('check both bots and apply', async () => {
      await rows.filter({ hasText: nameA }).click()
      await rows.filter({ hasText: nameB }).click()
      await sheet.getByRole('button', { name: 'Apply' }).click()
      await expect(rows.filter({ hasText: nameA })).toContainText('Applied', { timeout: 30_000 })
      await expect(rows.filter({ hasText: nameB })).toContainText('Applied', { timeout: 30_000 })
    })

    await test.step('an Apply clears what it changed from the selection', async () => {
      await expect(rows.filter({ hasText: nameA })).toHaveAttribute('aria-checked', 'false')
      await expect(rows.filter({ hasText: nameB })).toHaveAttribute('aria-checked', 'false')
    })

    await test.step('a second Apply starts fresh: only the newly checked bot changes', async () => {
      await rows.filter({ hasText: nameB }).click()
      await sheet.getByRole('button', { name: 'Apply' }).click()
      await expect(rows.filter({ hasText: nameB })).toContainText('Applied', { timeout: 30_000 })
      // A was applied by the first Apply and left the selection: this Apply must not touch it.
      await expect(rows.filter({ hasText: nameA })).not.toContainText('Applied')
      await expect(rows.filter({ hasText: nameA })).toHaveAttribute('aria-checked', 'false')
    })

    await test.step('apply touches only the bots the filter shows (the hidden-checked pin)', async () => {
      const search = sheet.getByRole('textbox', { name: 'Search bots' })
      // The Apply cleared the selection, so the pin starts from an empty one.
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

/** The provider filter narrows the sheet to one agent; what it hides is never touched. */
test('the provider filter narrows the Bot Manager to one agent, and what it hides stays untouched', async () => {
  test.setTimeout(120_000)
  const { browser, page } = await attach()
  await setPref(page, 'macAccountOnboardingCompleted', true)
  await setPref(page, 'withoutAccount', true)
  if (await page.locator('.welcome').count()) await page.reload()
  await expect(page.locator('.sidebar')).toBeVisible({ timeout: 15_000 })
  for (let i = 0; i < 4 && (await page.locator('.model-manager').count()) > 0; i++) {
    await page.locator('.model-manager').first().locator('.model-manager-head').getByRole('button').click().catch(() => {})
    await page.waitForTimeout(400)
  }
  await expect(page.locator('.model-manager')).toHaveCount(0)

  const run = Date.now()
  const nameCustom = `Provider custom ${run}`
  const nameAgent = `Provider agent ${run}`
  const created: string[] = []
  try {
    const { backends } = await api<Hello>(page, 'hello', {})
    const agent = backends.find((b) => b.available !== false && b.installed !== false && b.id !== 'custom')
    test.skip(!agent, 'no installed agent in this sandbox')

    // One bot on the custom command, one on the installed agent: two providers to filter between.
    created.push((await steerBot(page, nameCustom, 1)).id)
    created.push((await api<{ bot: { id: string } }>(page, 'createBot', { name: nameAgent, backend: agent!.id, connectors: [], skills: [], notify: false })).bot.id)

    await page.locator('button[title="Bot Manager"]').first().click()
    const sheet = page.locator('.model-manager')
    await expect(sheet).toBeVisible()
    const rows = sheet.locator('.model-manager-row')
    await expect(rows.filter({ hasText: nameCustom })).toHaveCount(1)
    await expect(rows.filter({ hasText: nameAgent })).toHaveCount(1)

    await test.step('the filter menu offers both providers, All providers first', async () => {
      const filter = sheet.locator('.model-manager-provider')
      await expect(filter).toBeVisible()
      await expect(filter).toContainText('All providers')
      await filter.click()
      await page.getByRole('menuitemradio', { name: 'All providers', exact: true }).click()
      // Picking All providers changes nothing: both rows stay.
      await expect(rows.filter({ hasText: nameCustom })).toHaveCount(1)
      await filter.click()
      await page.getByRole('menuitemradio', { name: agent!.name, exact: true }).click()
      await expect(rows.filter({ hasText: nameAgent })).toBeVisible()
      await expect(rows.filter({ hasText: nameCustom })).toHaveCount(0)
      // The button names the chosen provider.
      await expect(filter).toContainText(agent!.name)
    })

    await test.step('a checked bot the provider filter hides is not paused either', async () => {
      // Back to every provider first, so both bots can be checked while they show; nothing is
      // checked yet, so there is no Unselect all to do.
      const filter = sheet.locator('.model-manager-provider')
      await filter.click()
      await page.getByRole('menuitemradio', { name: 'All providers', exact: true }).click()
      await expect(rows.filter({ hasText: nameCustom })).toHaveCount(1)
      await rows.filter({ hasText: nameCustom }).click()
      await rows.filter({ hasText: nameAgent }).click()
      // Then narrow: the custom bot is hidden and checked.
      await filter.click()
      await page.getByRole('menuitemradio', { name: agent!.name, exact: true }).click()
      await expect(sheet.getByText('hidden by the filter')).toBeVisible()
      await sheet.getByRole('button', { name: 'Pause checked', exact: true }).click()
      await expect(rows.filter({ hasText: nameAgent })).toContainText('Paused', { timeout: 30_000 })
      // Back to every provider: the hidden bot was left exactly as it was.
      await filter.click()
      await page.getByRole('menuitemradio', { name: 'All providers', exact: true }).click()
      await expect(rows.filter({ hasText: nameCustom })).toHaveCount(1)
      await expect(rows.filter({ hasText: nameCustom })).not.toContainText('Paused', { timeout: 30_000 })
      // Clean up for whatever comes after: resume the paused bot.
      await sheet.getByRole('button', { name: 'Unselect all', exact: true }).click()
      await rows.filter({ hasText: nameAgent }).click()
      await sheet.getByRole('button', { name: 'Resume checked', exact: true }).click()
      await expect(rows.filter({ hasText: nameAgent })).not.toContainText('Paused', { timeout: 30_000 })
    })
  } finally {
    for (const id of created) await api(page, 'deleteBot', { botId: id }).catch(() => {})
    await browser.close()
  }
})

