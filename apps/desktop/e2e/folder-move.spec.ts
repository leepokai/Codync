import { test, expect, type Page } from '@playwright/test'
import { existsSync, mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { fixtureCommand } from './fixture-command.ts'
import { api, attach, sandbox, setPref, typeMessage } from './sandbox.ts'

// A scripted agent that answers "primary answered" and logs every request to primary.jsonl in its folder
// (host/tests/fixtures/relay_agent.py). Playwright loads specs as CommonJS here, so `__dirname` is defined.
const agent = join(resolve(__dirname, '../../..'), 'host/tests/fixtures/relay_agent.py')

const row = (page: Page, name: string) => page.locator('[data-roster-row]').filter({ has: page.getByText(name, { exact: true }) })

/** The last prompt the agent got while working in `dir` ('' before it got one). */
function lastPrompt(dir: string) {
  const log = join(dir, 'primary.jsonl')
  if (!existsSync(log)) return ''
  const prompts = readFileSync(log, 'utf8').trim().split('\n').map((l) => JSON.parse(l)).filter((r) => r.method === 'session/prompt')
  return (prompts.at(-1)?.params.prompt[0].text as string | undefined) ?? ''
}

test.beforeAll(async () => {
  const { browser, page } = await attach()
  await setPref(page, 'appLanguage', 'en')
  await page.reload()
  await browser.close()
})

test.afterAll(async () => {
  const { browser, page } = await attach()
  await page.evaluate(() => localStorage.removeItem('appLanguage'))
  await page.reload()
  await browser.close()
})

test("moving a bot to another folder carries the conversation to its next message, with no line in the chat", async () => {
  test.setTimeout(60_000)
  const { browser, page } = await attach()
  const name = `Folder move e2e ${Date.now()}`
  const project = mkdtempSync(join(tmpdir(), 'codync-move-'))
  let botId: string | undefined
  try {
    botId = (await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command: fixtureCommand(agent) + ' primary', cwd: project, connectors: [], skills: [], notify: false,
    })).bot.id
    await row(page, name).click()
    await typeMessage(page, 'Fix the login tests')
    await expect.poll(() => lastPrompt(project), { timeout: 20_000 }).toContain('Fix the login tests')
    await expect(page.locator('.bubble', { hasText: 'primary answered' })).toBeVisible({ timeout: 20_000 })
    const notices = await page.locator('.chat-row.notice').count()

    // Settings › Workspace: back to the bot's own folder.
    const details = page.locator('.details-panel')
    if (!(await details.isVisible())) await page.getByRole('button', { name: `${name}, view details` }).click()
    const chooser = details.locator('.panel-choice', { hasText: 'Settings' })
    await expect(chooser.or(details.getByRole('tab', { name: 'Settings' }))).toBeVisible()
    if (await chooser.isVisible()) await chooser.click()
    else await details.getByRole('tab', { name: 'Settings' }).click()
    await details.locator('.workspace-pill').click()
    await page.getByRole('menuitemradio', { name: 'Own folder' }).or(page.getByRole('menuitem', { name: 'Own folder' })).click()
    await expect(details.locator('.workspace-pill')).toHaveText('Own folder')
    // Saved (a moment after the change): the host reads the bot's own folder, a repository, back.
    await expect(details).toContainText('Git repository')
    const own = join(sandbox().home, 'bots', botId, 'workspace')

    await typeMessage(page, 'And the signup tests')
    await expect.poll(() => lastPrompt(own), { timeout: 20_000 }).toContain('And the signup tests')
    const prompt = lastPrompt(own)
    expect(prompt).toContain('<handoff>')
    expect(prompt).toContain('Fix the login tests')
    expect(prompt).not.toContain('another agent')
    await expect(page.locator('.bubble', { hasText: 'primary answered' })).toHaveCount(2, { timeout: 20_000 })
    await expect(page.locator('.chat-row.notice')).toHaveCount(notices)
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    rmSync(project, { recursive: true, force: true })
    await browser.close()
  }
})
