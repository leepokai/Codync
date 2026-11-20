import { test, expect, type Page } from '@playwright/test'
import { execFileSync } from 'node:child_process'
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { basename, join } from 'node:path'
import { fixtureCommand } from './fixture-command.ts'
import { api, attach, echoBot, sandbox, setPref, workAgent, typeMessage } from './sandbox.ts'

const row = (page: Page, name: string) => page.locator('[data-roster-row]').filter({ has: page.getByText(name, { exact: true }) })

/** The Settings tab of the details beside the bot's chat. */
async function openSettings(page: Page, name: string) {
  await row(page, name).click()
  const details = page.locator('.details-panel')
  if (!(await details.isVisible())) await page.getByRole('button', { name: `${name}, view details` }).click()
  await details.getByRole('tab', { name: 'Settings' }).click()
  return details
}

/** The versions of a folder, newest first (none while it is no repository). */
function versions(dir: string) {
  try {
    return execFileSync('git', ['-C', dir, 'log', '--format=%s'], { encoding: 'utf8', stdio: 'pipe' }).trim().split('\n')
  } catch {
    return []
  }
}

const MADE = 'Codync keeps versions of this folder'

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

test('a new bot works in its own folder: so named in the list, its chat and its settings, with no worktree switch', async () => {
  const { browser, page } = await attach()
  const name = `Own folder e2e ${Date.now()}`
  let botId: string | undefined
  try {
    await page.getByRole('button', { name: 'New', exact: true }).filter({ visible: true }).click()
    await page.getByRole('menuitem', { name: 'New bot' }).click()
    const sheet = page.locator('.create-sheet')
    await sheet.getByLabel('Name').fill(name)
    await sheet.getByRole('button', { name: 'Create', exact: true }).click()
    await expect(sheet).toHaveCount(0)
    await expect(row(page, name)).toBeVisible()
    botId = (await row(page, name).getAttribute('data-roster-row'))?.split('/')[1]

    await expect(row(page, name)).toContainText(' · Own folder')
    await row(page, name).click()
    await expect(page.locator('.intro-card-path')).toHaveText(/ · Own folder$/)

    // Codync makes its folder a repository to keep versions in, and every agent works there: no worktrees.
    const details = await openSettings(page, name)
    await expect(details.locator('.workspace-pill')).toHaveText('Own folder')
    await expect(details).toContainText('Git repository')
    await expect(details.getByText('Worktree per thread')).toHaveCount(0)
    expect(versions(join(sandbox().home, 'bots', botId!, 'workspace'))).toEqual([MADE])
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})

test("a turn in the bot's own folder leaves a version before and after it", async () => {
  const { browser, page } = await attach()
  const name = `Own folder turn e2e ${Date.now()}`
  let botId: string | undefined
  try {
    // On the scripted agent that logs each prompt in its folder (not `workBot`, whose settings change would race the
    // folder's first version).
    botId = (await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command: fixtureCommand(workAgent), connectors: [], skills: [], notify: false,
    })).bot.id
    const dir = join(sandbox().home, 'bots', botId, 'workspace')
    await expect.poll(() => versions(dir)).toEqual([MADE])
    writeFileSync(join(dir, 'notes.md'), 'before the turn')
    await row(page, name).click()
    await typeMessage(page, 'Tidy up the notes')
    await expect.poll(() => versions(dir)).toEqual(['Tidy up the notes', 'Before: Tidy up the notes', MADE])
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})

test('a project folder in a git repository keeps its worktree switch and its own history', async () => {
  const { browser, page } = await attach()
  const name = `Project folder e2e ${Date.now()}`
  const dir = mkdtempSync(join(tmpdir(), 'codync-project-'))
  execFileSync('git', ['-C', dir, 'init', '-q'])
  let botId: string | undefined
  try {
    botId = (await echoBot(page, name, dir)).id
    await expect(row(page, name)).toContainText(` · ${basename(dir)}`)
    const details = await openSettings(page, name)
    await expect(details.locator('.workspace-pill')).toHaveText(basename(dir))
    await expect(details).toContainText('Git repository')
    await expect(details.getByText('Worktree per thread')).toBeVisible()
    expect(versions(dir)).toEqual([])
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    rmSync(dir, { recursive: true, force: true })
    await browser.close()
  }
})
