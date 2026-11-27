import { test, expect, type Page } from '@playwright/test'
import { execFileSync } from 'node:child_process'
import { existsSync, lstatSync, mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { api, attach, openPage, sandbox, setPref, workBot } from './sandbox.ts'

// Stage 7 of docs/plans/bots-projects-threads.md: a git project's Shared folders, a new thread's place, and a thread
// without a bot in a folder of its own.

const row = (page: Page, name: string) => page.locator('[data-roster-row]').filter({ has: page.getByText(name, { exact: true }) })
type Saved = { projects: string[]; sharedFolders: Record<string, string[]> | null }

/** A git repository with one commit that ignores `.env` and `node_modules/`, lists `.env` in `.worktreeinclude`, and
 * has both in its checkout. */
function repo() {
  const dir = mkdtempSync(join(tmpdir(), 'codync-shared-'))
  const git = (...args: string[]) => execFileSync('git', ['-C', dir, ...args], { stdio: 'pipe' })
  git('init', '-q')
  writeFileSync(join(dir, '.gitignore'), '.env\nnode_modules/\n')
  writeFileSync(join(dir, '.worktreeinclude'), '.env\n')
  git('add', '.')
  git('-c', 'user.name=e2e', '-c', 'user.email=e2e@e2e', 'commit', '-q', '-m', 'ignores')
  writeFileSync(join(dir, '.env'), 'KEY=1')
  mkdirSync(join(dir, 'node_modules/pkg'), { recursive: true })
  writeFileSync(join(dir, 'node_modules/pkg/index.js'), 'module.exports = 1')
  return dir
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

test("a git project's Shared folders are added and removed in bot settings and saved with the bot", async () => {
  test.setTimeout(60_000)
  const { browser, page } = await attach()
  const name = `Shared folders e2e ${Date.now()}`
  const project = repo()
  let botId: string | undefined
  const saved = async () => (await api<{ bot: Saved }>(page, 'updateBot', { id: botId })).bot.sharedFolders ?? {}
  try {
    botId = (await workBot(page, name)).id
    await api(page, 'updateBot', { id: botId, projects: [project] })
    await row(page, name).click()
    const details = page.locator('.details-panel')
    if (!(await details.isVisible())) await page.getByRole('button', { name: `${name}, view details` }).click()
    await details.getByRole('tab', { name: 'Settings' }).click()

    await details.getByRole('button', { name: 'Shared folders' }).click()
    const sheet = page.locator('.shared-folders-body')
    await sheet.getByRole('textbox', { name: 'Folder name' }).fill('./node_modules/')
    await sheet.getByRole('textbox', { name: 'Folder name' }).press('Enter')
    await expect(sheet.getByText('node_modules', { exact: true })).toBeVisible()
    await expect.poll(saved).toEqual({ [project]: ['node_modules'] })

    await sheet.getByRole('button', { name: 'Remove folder' }).click()
    await expect(sheet.getByText('node_modules', { exact: true })).toHaveCount(0)
    await expect.poll(saved).toEqual({})
    await page.keyboard.press('Escape')
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    rmSync(project, { recursive: true, force: true })
    await browser.close()
  }
})

test('a thread started in a New worktree has the shared folders linked and the .worktreeinclude files copied', async () => {
  test.setTimeout(90_000)
  const { browser, page } = await attach()
  const name = `New worktree e2e ${Date.now()}`
  const project = repo()
  let botId: string | undefined
  try {
    botId = (await workBot(page, name)).id
    await api(page, 'updateBot', { id: botId, projects: [project], worktreeMode: 'off', sharedFolders: { [project]: ['node_modules'] } })
    await openPage(page, 'Threads')
    await page.locator('.sidebar-header').getByRole('button', { name: 'New thread', exact: true }).click()
    const start = page.locator('.new-thread')
    await start.locator('.thread-option[title="Choose a bot"]').click()
    await page.getByRole('menuitemradio', { name, exact: true }).click()
    // The bot's mode is Off: Here until another place is picked.
    const place = start.locator('.thread-option[title="Worktree mode"]')
    await expect(place).toContainText('Here')
    await place.click()
    await page.getByRole('menuitemradio', { name: 'New worktree' }).click()
    await expect(place).toContainText('New worktree')
    await start.getByRole('textbox').fill('Build it HOLD:e2e')
    await start.getByRole('textbox').press('Enter')

    const worktrees = join(sandbox().home, 'worktrees', botId)
    const made = () => (existsSync(worktrees) ? readdirSync(worktrees).map((task) => join(worktrees, task)) : [])
    await expect.poll(() => made().some((dir) => existsSync(join(dir, '.env'))), { timeout: 30_000 }).toBe(true)
    const worktree = made()[0]
    expect(lstatSync(join(worktree, 'node_modules')).isSymbolicLink()).toBe(true)
    expect(existsSync(join(worktree, 'node_modules/pkg/index.js'))).toBe(true)
    // Git passes over the link, so the thread can't commit it.
    expect(execFileSync('git', ['-C', worktree, 'status', '--porcelain'], { encoding: 'utf8' })).not.toContain('node_modules')
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await openPage(page, 'Bots').catch(() => {})
    rmSync(project, { recursive: true, force: true })
    await browser.close()
  }
})

test('a thread without a bot starts in a folder of its own', async () => {
  test.setTimeout(60_000)
  const { browser, page } = await attach()
  const threads = join(sandbox().home, 'threads')
  const before = new Set(existsSync(threads) ? readdirSync(threads) : [])
  const text = `Say hello ${Date.now()}`
  try {
    await openPage(page, 'Threads')
    await page.locator('.sidebar-header').getByRole('button', { name: 'New thread', exact: true }).click()
    const start = page.locator('.new-thread')
    const bots = start.locator('.thread-option[title="Choose a bot"]')
    if (!(await bots.textContent())?.includes('No bot')) {
      await bots.click()
      await page.getByRole('menuitemradio', { name: 'No bot', exact: true }).click()
    }
    await expect(start.locator('.thread-option[title="Workspace"]')).toContainText('Own folder')
    await start.getByRole('textbox').fill(text)
    await start.getByRole('textbox').press('Enter')
    await expect.poll(() => (existsSync(threads) ? readdirSync(threads) : []).filter((task) => !before.has(task)).length, { timeout: 20_000 }).toBe(1)
    // Its folder is named "Own folder", never by its id, where it works and in the Threads list, all such threads in one group.
    const where = page.locator('.work-where-copy').filter({ visible: true }).first()
    await expect(where).toHaveText('Own folder')
    await expect(where).toHaveAttribute('aria-label', /threads[\\/][0-9a-f-]{36}$/)
    const groups = page.locator('.thread-group[aria-label="Own folder"]')
    await expect(groups).toHaveCount(1)
    await expect(groups.getByText(text)).toBeVisible()
    await expect(page.locator('.thread-group').filter({ hasText: /[0-9a-f]{8}-[0-9a-f]{4}-/ })).toHaveCount(0)
  } finally {
    // ponytail: the ended thread stays in the sandbox's Recent (and its folder until the task leaves the box): no API
    // names the hidden box of threads without a bot to drop it; add one if another spec needs a clean list.
    await openPage(page, 'Bots').catch(() => {})
    await browser.close()
  }
})
