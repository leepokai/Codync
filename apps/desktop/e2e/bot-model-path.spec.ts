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
    await page.locator('.model-manager').first().locator('.modal-header').getByRole('button', { name: 'Close' }).click({ timeout: 4000 }).catch(() => {})
    await page.waitForTimeout(400)
  }
  await expect(page.locator('.model-manager')).toHaveCount(0)
  return attached
}

/**
 * The Bot Manager shows each bot's agent and model under its name, and its search can read them:
 * a query for the model finds the bot that runs it, and the switch turns that off again.
 */
test("the Bot Manager lists each bot's model path, and its search reads the model when asked", async () => {
  test.setTimeout(120_000)
  const { browser, page } = await appPage()
  const run = Date.now()
  const nameOne = `Model path e2e one ${run}`
  const nameTwo = `Model path e2e two ${run}`
  const created: string[] = []
  try {
    const { backends } = await api<Hello>(page, 'hello', {})
    const agent = backends.find((b) => b.available !== false && b.installed !== false && b.id !== 'custom')
    test.skip(!agent, 'no installed agent in this sandbox')

    // Two bots on the same agent with different models: only the model tells them apart.
    const one = await api<{ bot: { id: string } }>(page, 'createBot', {
      name: nameOne, backend: agent!.id, model: 'opendesign/deepseek-v4.1-flash-eco', connectors: [], skills: [], notify: false,
    })
    created.push(one.bot.id)
    const two = await api<{ bot: { id: string } }>(page, 'createBot', {
      name: nameTwo, backend: agent!.id, model: 'opendesign/hy4-preview', connectors: [], skills: [], notify: false,
    })
    created.push(two.bot.id)

    await page.locator('button[title="Bot Manager"]').first().click()
    const sheet = page.locator('.model-manager')
    await expect(sheet).toBeVisible()
    const rows = sheet.locator('.model-manager-row')
    await expect(rows.filter({ hasText: nameOne })).toHaveCount(1)
    await expect(rows.filter({ hasText: nameTwo })).toHaveCount(1)
    // The field by where it sits, not by its prompt: the prompt changes with the switch.
    // The folded sheet uses the shared palette field here, not a .search-field.
    const search = sheet.locator('.model-manager-body .palette-field input')

    await test.step('each row names the agent it runs on and its model id, verbatim', async () => {
      // The agent as hello names it, then the model id exactly as stored. The fold joins
      // them with `->` (rowModelPath); the original spec used a middle dot.
      const line = rows.filter({ hasText: nameOne }).locator('.model-manager-model-path')
      await expect(line).toHaveCount(1)
      await expect(line).toHaveText(`${agent!.name}->opendesign/deepseek-v4.1-flash-eco`)
      await expect(rows.filter({ hasText: nameTwo }).locator('.model-manager-model-path')).toHaveText(`${agent!.name}->opendesign/hy4-preview`)
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
      await search.fill('')
      await expect(rows.filter({ hasText: nameTwo })).toHaveCount(1)
    })

    await test.step('switched off, the search reads names alone: the same query finds nothing', async () => {
      const toggle = sheet.locator('.model-manager-model-search')
      await expect(toggle).toBeVisible()
      await expect(toggle).toHaveAttribute('aria-checked', 'true')
      await toggle.click()
      await expect(toggle).toHaveAttribute('aria-checked', 'false')
      // The prompt says what the search covers now, so the field never promises more than it does.
      await expect(search).toHaveAttribute('aria-label', 'Search bots')
      await search.fill('deepseek')
      await expect(rows.filter({ hasText: nameOne })).toHaveCount(0)
      await expect(rows.filter({ hasText: nameTwo })).toHaveCount(0)
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
