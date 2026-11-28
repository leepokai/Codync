import { test, expect, type Page } from '@playwright/test'
import { execFileSync } from 'node:child_process'
import { existsSync, mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { basename, join, resolve } from 'node:path'
import { fixtureCommand } from './fixture-command.ts'
import { api, attach, openDetails, sandbox, setPref, typeMessage } from './sandbox.ts'

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

test("adding a project in Settings leaves the chat in the bot's own folder, in the same session", async () => {
  test.setTimeout(60_000)
  const { browser, page } = await attach()
  const name = `Projects e2e ${Date.now()}`
  const project = mkdtempSync(join(tmpdir(), 'codync-project-'))
  let botId: string | undefined
  try {
    botId = (await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command: fixtureCommand(agent) + ' primary', connectors: [], skills: [], notify: false,
    })).bot.id
    const own = join(sandbox().home, 'bots', botId, 'workspace')
    await row(page, name).click()
    await typeMessage(page, 'Fix the login tests')
    await expect.poll(() => lastPrompt(own), { timeout: 20_000 }).toContain('Fix the login tests')
    await expect(page.locator('.bubble', { hasText: 'primary answered' })).toBeVisible({ timeout: 20_000 })

    // Settings › Workspace: the bot's own folder, read-only, and its projects.
    const details = await openDetails(page, 'Settings')
    await expect(details.getByText("Bot's folder")).toBeVisible()
    await expect(details.getByText(own, { exact: true })).toBeVisible()
    await expect(details.getByText('Worktree per thread')).toHaveCount(0)

    // Add one with the folder picker: listed as the default, the chat stays where it is.
    await details.getByRole('button', { name: 'Add project' }).click()
    const browse = page.getByRole('button', { name: 'Browse other folders' })
    if (await browse.isVisible()) await browse.click()
    await page.getByRole('button', { name: 'Enter folder path' }).click()
    await page.getByRole('textbox', { name: 'Folder path' }).fill(project)
    await page.getByRole('button', { name: 'Go to folder' }).click()
    await page.getByRole('button', { name: 'Use this folder' }).click()
    await expect(details.getByText(project, { exact: true })).toBeVisible()
    // On the project's row: the model picker can read "Default" too.
    await expect(details.locator('.fade-in', { has: page.getByText(project, { exact: true }) }).last().getByText('Default', { exact: true })).toBeVisible()
    // Not a git repository: no worktree mode.
    await expect(details.getByText('Worktree mode')).toHaveCount(0)

    await typeMessage(page, 'And the signup tests')
    await expect.poll(() => lastPrompt(own), { timeout: 20_000 }).toContain('And the signup tests')
    expect(lastPrompt(own)).not.toContain('<handoff>')
    expect(existsSync(join(project, 'primary.jsonl'))).toBe(false)

    // Removed again: the list is empty.
    await details.getByRole('button', { name: 'Remove project' }).click()
    await expect(details.getByText(project, { exact: true })).toHaveCount(0)
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    rmSync(project, { recursive: true, force: true })
    await browser.close()
  }
})

/** A git repository in a new temporary folder named `codync-<name>-…`. */
function gitRepo(name: string) {
  const dir = mkdtempSync(join(tmpdir(), `codync-${name}-`))
  execFileSync('git', ['init', '-q'], { cwd: dir })
  return dir
}

test('with projects in git repositories: Make default, the worktree mode, and a new thread lists the projects first', async () => {
  test.setTimeout(60_000)
  const { browser, page } = await attach()
  const name = `Projects git e2e ${Date.now()}`
  const [first, second] = [gitRepo('first'), gitRepo('second')]
  let botId: string | undefined
  const host = async () => (await api<{ bot: { projects: string[]; worktreeMode: string; cwd: string } }>(page, 'updateBot', { id: botId })).bot
  try {
    botId = (await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command: fixtureCommand(agent) + ' primary', projects: [first, second], connectors: [], skills: [], notify: false,
    })).bot.id
    await expect(row(page, name)).toContainText(` · ${basename(first)}`)
    await row(page, name).click()
    const details = await openDetails(page, 'Settings')

    // Worktree mode: shown for a project in a git repository, Side by side until another is picked.
    await expect(details.getByText('Worktree mode')).toBeVisible()
    await details.locator('.choice-label', { hasText: 'Side by side' }).click()
    await page.getByRole('menuitemradio', { name: /^Always/ }).click()
    await expect(details.locator('.choice-label', { hasText: 'Always' })).toBeVisible()
    await expect.poll(async () => (await host()).worktreeMode).toBe('always')

    // Make default: the second project moves first, and the list and the roster follow.
    await details.getByRole('button', { name: 'Make default' }).click()
    await expect.poll(async () => (await host()).projects).toEqual([second, first])
    await expect(row(page, name)).toContainText(` · ${basename(second)}`)

    // A new thread from the Threads page: its folder chip lists the projects (the default first), then the bot's folder.
    await page.locator('.nav-rail').getByRole('button', { name: 'Threads', exact: true }).click()
    await page.locator('aside.sidebar .sidebar-header-action[aria-label="New thread"]').click()
    const start = page.locator('.new-thread')
    const bots = start.locator('.thread-option[title="Choose a bot"]')
    if (await bots.isEnabled()) {
      await bots.click()
      await page.getByRole('menuitemradio', { name, exact: true }).click()
    }
    await expect(start).toContainText(`New thread with ${name}`)
    const workspace = start.locator('.thread-option[title="Workspace"]')
    await expect(workspace).toContainText(basename(second))
    await workspace.click()
    const folders = page.getByRole('menuitemradio')
    await expect(folders.nth(0)).toContainText(basename(second))
    await expect(folders.nth(1)).toContainText(basename(first))
    await expect(folders.nth(2)).toContainText('Own folder')
    await page.keyboard.press('Escape')
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    for (const dir of [first, second]) rmSync(dir, { recursive: true, force: true })
    await browser.close()
  }
})
