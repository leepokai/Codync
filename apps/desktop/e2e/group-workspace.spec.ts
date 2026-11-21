import { test, expect, type Page } from '@playwright/test'
import { readFileSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { basename, join, resolve } from 'node:path'
import { fixtureCommand } from './fixture-command.ts'
import { api, attach, sandbox, setPref, typeMessage } from './sandbox.ts'

// A scripted agent that logs every prompt in its folder and answers the room (host/tests/fixtures/team_agent.py).
const teamAgent = join(resolve(__dirname, '../../..'), 'host/tests/fixtures/team_agent.py')

const row = (page: Page, name: string) =>
  page.locator('[data-roster-row]').filter({ has: page.getByText(name, { exact: true }) })

/** Opens the group's editor from its details panel (its member list's Add Member row). */
async function openGroupEditor(page: Page, name: string) {
  await row(page, name).click()
  const details = page.locator('.details-panel')
  // A wide window keeps the panel closed until its title is clicked; a narrow one asks for the details sheet.
  if (!(await details.isVisible())) await page.locator('.title-pill').click()
  await details.locator('.member-row').last().click()
  return page.locator('.group-form')
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

test('a group keeps a shared workspace folder and its members are told about it', async () => {
  // A room turn on the scripted agents, then their prompt log, can take a little longer than the suite's 30 s.
  test.setTimeout(90_000)
  const { browser, page } = await attach()
  const made: string[] = []
  const dir = mkdtempSync(join(tmpdir(), 'codync-team-'))
  try {
    const create = async (body: object) => {
      const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', body)
      made.push(bot.id)
      return bot.id
    }
    const command = fixtureCommand(teamAgent)
    const alpha = await create({ name: `Team Alpha ${Date.now()}`, backend: 'custom', command, connectors: [], skills: [], notify: false, permission: 'auto' })
    const beta = await create({ name: `Team Beta ${Date.now()}`, backend: 'custom', command, connectors: [], skills: [], notify: false, permission: 'auto' })
    const name = `Team Room ${Date.now()}`
    await create({ name, kind: 'group', members: [alpha, beta] })

    // The editor offers the folder and starts on each keeping its own.
    let form = await openGroupEditor(page, name)
    await expect(form.locator('.workspace-pill')).toHaveText('Each keeps its own folder')

    // Pick the team's folder through the shared folder picker.
    await form.locator('.workspace-pill').click()
    await page.getByRole('menuitemradio', { name: 'Choose project folder…' }).click()
    await page.getByRole('button', { name: 'Browse other folders' }).click()
    await page.getByRole('button', { name: 'Enter folder path' }).click()
    await page.locator('.workspace-path input').fill(dir)
    await page.getByRole('button', { name: 'Go to folder' }).click()
    await expect(page.locator('.workspace-footer')).toContainText(dir)
    await page.getByRole('button', { name: 'Use this folder' }).click()
    await expect(form.locator('.workspace-pill')).toHaveText(basename(dir))

    await page.getByRole('button', { name: 'Save', exact: true }).click()
    await expect(form).toHaveCount(0)

    // It survives a reload: the host keeps it.
    await page.reload()
    form = await openGroupEditor(page, name)
    await expect(form.locator('.workspace-pill')).toHaveText(basename(dir))
    await page.getByRole('button', { name: 'Save', exact: true }).click()
    await expect(form).toHaveCount(0)

    // A member's room turn is told where the team's folder is.
    await typeMessage(page, 'hello team')
    const prompts = () => {
      try {
        return readFileSync(join(sandbox().home, 'bots', alpha, 'workspace', 'prompts.jsonl'), 'utf8')
      } catch {
        return ''
      }
    }
    await expect.poll(prompts, { timeout: 60_000 }).toContain(`The team shares the folder ${dir}`)
  } finally {
    for (const botId of made.reverse()) await api(page, 'deleteBot', { botId }).catch(() => {})
    rmSync(dir, { recursive: true, force: true })
    await browser.close()
  }
})
