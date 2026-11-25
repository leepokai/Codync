import { test, expect } from '@playwright/test'
import { api, attach, setPref } from './sandbox.ts'

type Hello = { backends: { id: string; name: string; available?: boolean; installed?: boolean }[] }

/** The sandbox app past the welcome, with no sheet left open by a previous run. */
async function appPage() {
  const attached = await attach()
  const { page } = attached
  await setPref(page, 'macAccountOnboardingCompleted', true)
  await setPref(page, 'withoutAccount', true)
  if (await page.locator('.welcome').count()) await page.reload()
  await expect(page.locator('.welcome')).toHaveCount(0, { timeout: 15_000 })
  await expect(page.locator('.sidebar')).toBeVisible({ timeout: 15_000 })
  for (let i = 0; i < 4 && (await page.locator('.model-manager').count()) > 0; i++) {
    await page.keyboard.press('Escape').catch(() => {})
    await page.locator('.model-manager').first().locator('.model-manager-head').getByRole('button').click({ timeout: 4000 }).catch(() => {})
    await page.waitForTimeout(400)
  }
  await expect(page.locator('.model-manager')).toHaveCount(0)
  return attached
}

/**
 * The Bot Manager shows each bot's agent and model under its name, its search can read them, and a
 * path too long for the row shows whole on hover: a query for the model finds the bot that runs it,
 * the switch turns that off again, and the hover tip only appears when the line is really clipped.
 */
test("the Bot Manager lists each bot's model path, and its search reads the model when asked", async () => {
  test.setTimeout(120_000)
  const { browser, page } = await appPage()
  const run = Date.now()
  const nameOne = `Model path e2e one ${run}`
  const nameTwo = `Model path e2e two ${run}`
  const nameLong = `Model path e2e long ${run}`
  const created: string[] = []
  // Long enough that the row cannot show it at the sheet's opening width, and still too long when
  // the sheet is dragged as wide as the window allows.
  const longModel = 'opendesign/deepseek-v4.1-flash-eco-codync-preview-candidate-2026-11-25-with-a-very-long-tail-name'
  try {
    const { backends } = await api<Hello>(page, 'hello', {})
    const agent = backends.find((b) => b.available !== false && b.installed !== false && b.id !== 'custom')
    test.skip(!agent, 'no installed agent in this sandbox')

    // Two bots on the same agent with different models: only the model tells them apart. The third
    // runs a model id far longer than a row, to check the hover tip.
    const one = await api<{ bot: { id: string } }>(page, 'createBot', {
      name: nameOne, backend: agent!.id, model: 'opendesign/deepseek-v4.1-flash-eco', connectors: [], skills: [], notify: false,
    })
    created.push(one.bot.id)
    const two = await api<{ bot: { id: string } }>(page, 'createBot', {
      name: nameTwo, backend: agent!.id, model: 'opendesign/hy4-preview', connectors: [], skills: [], notify: false,
    })
    created.push(two.bot.id)
    const long = await api<{ bot: { id: string } }>(page, 'createBot', {
      name: nameLong, backend: agent!.id, model: longModel, connectors: [], skills: [], notify: false,
    })
    created.push(long.bot.id)

    await page.locator('button[title="Bot Manager"]').first().click()
    const sheet = page.locator('.model-manager')
    await expect(sheet).toBeVisible()
    const rows = sheet.locator('.model-manager-row')
    await expect(rows.filter({ hasText: nameOne })).toHaveCount(1)
    await expect(rows.filter({ hasText: nameTwo })).toHaveCount(1)
    await expect(rows.filter({ hasText: nameLong })).toHaveCount(1)
    // The field by where it sits, not by its prompt: the prompt changes with the switch.
    const search = sheet.locator('.model-manager-body .search-field input')

    await test.step('each row names the agent it runs on and its model id, verbatim', async () => {
      // The agent as hello names it, then the model id exactly as the computer stores it (which is
      // the agent's own `provider/model`, so the provider is inside it).
      const line = rows.filter({ hasText: nameOne }).locator('.model-manager-model-path')
      await expect(line).toHaveCount(1)
      await expect(line).toHaveText(`${agent!.name} · opendesign/deepseek-v4.1-flash-eco`)
      await expect(rows.filter({ hasText: nameTwo }).locator('.model-manager-model-path')).toHaveText(`${agent!.name} · opendesign/hy4-preview`)
      // The shared `modelPath` the roster's hover tip draws puts ` · ` between the segments.
      await expect(rows.filter({ hasText: nameLong }).locator('.model-manager-model-path')).toHaveText(`${agent!.name} · ${longModel}`)
    })

    await test.step('a path the row clips shows whole on hover; one that fits shows nothing', async () => {
      // The tip is the app's own `title`, the same attribute the roster row's tip uses — no second
      // tooltip mechanism. It is set only while the line really is clipped (scrollWidth wider than
      // clientWidth), which is measured on the box, not guessed from the string's length.
      const clipped = rows.filter({ hasText: nameLong }).locator('.model-manager-model-path')
      await expect.poll(async () => clipped.evaluate((el) => el.scrollWidth > el.clientWidth)).toBe(true)
      await expect(clipped).toHaveAttribute('title', `${agent!.name} · ${longModel}`)
      // The words themselves are unchanged: the tip adds a way to read the end, it does not
      // replace the text or the row's accessible name.
      await expect(clipped).toHaveText(`${agent!.name} · ${longModel}`)

      // A path that fits needs no tip, so it carries none.
      const fits = rows.filter({ hasText: nameTwo }).locator('.model-manager-model-path')
      await expect.poll(async () => fits.evaluate((el) => el.scrollWidth > el.clientWidth)).toBe(false)
      await expect(fits).not.toHaveAttribute('title', /.+/)

      // The sheet is draggable, so the measurement has to follow it: dragged wide enough that the
      // long path fits, the tip goes away; dragged back to the narrow width, it returns. The hook
      // re-measures on resize — it does not latch on first paint.
      const edge = sheet.locator('.model-manager-resize.mm-right')
      await edge.focus()
      for (let i = 0; i < 60; i++) await edge.press('ArrowRight')
      await expect.poll(async () => clipped.evaluate((el) => el.scrollWidth > el.clientWidth)).toBe(false)
      await expect(clipped).not.toHaveAttribute('title', /.+/)
      for (let i = 0; i < 90; i++) await edge.press('ArrowLeft')
      await expect.poll(async () => clipped.evaluate((el) => el.scrollWidth > el.clientWidth)).toBe(true)
      await expect(clipped).toHaveAttribute('title', `${agent!.name} · ${longModel}`)
      // And the short path still carries no tip at the narrowest the sheet gets: a tip means the
      // line clips, never that the row happens to be narrow.
      await expect(fits).not.toHaveAttribute('title', /.+/)
    })

    await test.step('the search finds a bot by its model while the model switch is on', async () => {
      await expect(search).toHaveAttribute('aria-label', 'Search bots or models')
      await search.fill('deepseek')
      await expect(rows.filter({ hasText: nameOne })).toHaveCount(1)
      await expect(rows.filter({ hasText: nameTwo })).toHaveCount(0)
      // A model id is matched as it is printed and as it is usually typed.
      await search.fill('deepseek v4.1 flash eco')
      await expect(rows.filter({ hasText: nameOne })).toHaveCount(1)
      await expect(rows.filter({ hasText: nameTwo })).toHaveCount(0)
      // The provider part alone finds it too.
      await search.fill('opendesign')
      await expect(rows.filter({ hasText: nameOne })).toHaveCount(1)
      await expect(rows.filter({ hasText: nameTwo })).toHaveCount(1)
      await expect(rows.filter({ hasText: nameLong })).toHaveCount(1)
      await search.fill('')
      await expect(rows.filter({ hasText: nameTwo })).toHaveCount(1)
    })

    await test.step('switched off, the search reads names alone: the same query finds nothing', async () => {
      const toggle = sheet.locator('.model-manager-model-search')
      await expect(toggle).toBeVisible()
      await expect(toggle).toHaveAttribute('aria-checked', 'true')
      // The switch's own name is readable, not squeezed to nothing by the shared switch's
      // full-width label: the band beside the agent filter sizes it to its label.
      const label = toggle.locator('.switch-label')
      await expect(label).toHaveText('Search models too')
      await expect(label).toBeVisible()
      await toggle.click()
      await expect(toggle).toHaveAttribute('aria-checked', 'false')
      // The prompt says what the search covers now, so the field never promises more than it does.
      await expect(search).toHaveAttribute('aria-label', 'Search bots')
      await search.fill('deepseek')
      await expect(rows.filter({ hasText: nameOne })).toHaveCount(0)
      await expect(rows.filter({ hasText: nameTwo })).toHaveCount(0)
      await expect(rows.filter({ hasText: nameLong })).toHaveCount(0)
      // A bot's own name still finds it: only the model words are gone.
      await search.fill(nameOne)
      await expect(rows.filter({ hasText: nameOne })).toHaveCount(1)
      await toggle.click()
      await expect(toggle).toHaveAttribute('aria-checked', 'true')
      await expect(search).toHaveAttribute('aria-label', 'Search bots or models')
      await search.fill('deepseek')
      await expect(rows.filter({ hasText: nameOne })).toHaveCount(1)
    })
  } finally {
    for (const id of created) await api(page, 'deleteBot', { botId: id }).catch(() => {})
    await browser.close()
  }
})
