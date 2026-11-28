import { test, expect, type Page } from '@playwright/test'
import { existsSync, mkdtempSync, readFileSync, realpathSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { basename, join } from 'node:path'
import { fixtureCommand } from './fixture-command.ts'
import { api, attach, openDetails, openPage, setPref, workAgent } from './sandbox.ts'

type Saved = { routines: { id: string; name: string; folder?: string | null }[] }

/** The routine's editor, from the bot's details. */
async function openEditor(page: Page, bot: string, routine: string) {
  await page.locator('.roster-row', { hasText: bot }).click()
  await openDetails(page, 'Details')
  await page.locator('.routines-card', { hasText: routine }).locator('.routine-row').click()
  return page.locator('.routine-editor')
}

test("a routine's folder is picked in its editor, its run works there, and clearing it puts it back to the bot's", async () => {
  const { browser, page } = await attach()
  const name = `Routine folder e2e ${Date.now()}`
  const dir = realpathSync(mkdtempSync(join(tmpdir(), 'codync-routine-folder-')))
  let botId: string | undefined
  try {
    await setPref(page, 'appLanguage', 'en')
    await page.reload()
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command: fixtureCommand(workAgent), connectors: [], skills: [], notify: false,
    })
    botId = bot.id
    const { routine } = await api<{ routine: { id: string } }>(page, 'saveRoutine', {
      botId, name: 'Elsewhere', instruction: 'Report', triggers: [{ type: 'webhook' }],
    })
    const folderOf = async () => (await api<Saved>(page, 'routines', { botId })).routines.find((r) => r.id === routine.id)?.folder ?? null

    const editor = await openEditor(page, name, 'Elsewhere')
    const folderRow = editor.locator('.routine-folder')
    await expect(folderRow).toHaveText("Bot's folder")
    await folderRow.click()
    const picker = page.locator('.workspace-picker')
    const browse = picker.getByRole('button', { name: 'Browse other folders' })
    if (await browse.isVisible()) await browse.click()
    await picker.getByRole('button', { name: 'Enter folder path' }).click()
    await picker.getByLabel('Folder path', { exact: true }).and(page.locator('input')).fill(dir)
    await page.keyboard.press('Enter')
    await page.getByRole('button', { name: 'Use this folder' }).click()
    await expect(folderRow).toHaveText(basename(dir))
    await editor.getByRole('button', { name: 'Save changes' }).click()
    await expect.poll(folderOf).toBe(dir)

    // Run now: it works in that folder, in a thread of its own.
    await api(page, 'runRoutine', { botId, id: routine.id })
    const prompts = join(dir, 'prompts.jsonl')
    await expect.poll(() => existsSync(prompts) && readFileSync(prompts, 'utf8').includes('Scheduled routine: Elsewhere'), { timeout: 30_000 }).toBe(true)
    // Its result reaches the bot's chat.
    await expect(page.locator('.bubble', { hasText: /^Routine done:/ })).toBeVisible({ timeout: 20_000 })

    // Saved, the editor closed back to the bot's routines.
    await page.locator('.routines-card', { hasText: 'Elsewhere' }).locator('.routine-row').click()
    await expect(editor.locator('.routine-folder')).toHaveText(basename(dir))
    await editor.getByRole('button', { name: "Use the bot's folder" }).click()
    await expect(editor.locator('.routine-folder')).toHaveText("Bot's folder")
    await editor.getByRole('button', { name: 'Save changes' }).click()
    await expect.poll(folderOf).toBe(null)

    // The run is a thread on the Threads page, under its bot.
    await openPage(page, 'Threads')
    await page.locator('.sidebar-header .computer-filter').click()
    const byBot = page.getByRole('menuitemradio', { name: 'Group by bot', exact: true })
    if ((await byBot.getAttribute('aria-checked')) !== 'true') await byBot.click()
    await page.keyboard.press('Escape')
    await expect(page.locator('aside.sidebar').getByRole('group', { name, exact: true }).locator('[data-thread-row="Elsewhere"]')).toBeVisible()
    await openPage(page, 'Bots')
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    rmSync(dir, { recursive: true, force: true })
    await page.evaluate(() => localStorage.removeItem('appLanguage'))
    await page.reload()
    await browser.close()
  }
})
