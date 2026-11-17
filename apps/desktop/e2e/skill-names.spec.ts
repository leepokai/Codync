import { test, expect } from '@playwright/test'
import { api, attach, echoBot } from './sandbox.ts'

test("a bot's Skills section lists names only; the info button shows the description", async () => {
  const { browser, page } = await attach()
  const name = `Skill names e2e ${Date.now()}`
  const description = `Use when the e2e checks skill rows ${Date.now()}`
  let skillId: string | undefined
  let botId: string | undefined
  try {
    const { skill } = await api<{ skill: { id: string } }>(page, 'installSkill', { name, description, instructions: 'Say hi.' })
    skillId = skill.id
    const botName = `Skills e2e ${Date.now()}`
    botId = (await echoBot(page, botName)).id
    await page.locator('.roster-row', { hasText: botName }).click({ button: 'right' })
    await page.getByRole('menuitem', { name: 'Edit Profile…' }).click()

    const row = page.locator('.plugin-toggle', { hasText: name })
    await expect(row).toBeVisible()
    await expect(row).not.toContainText(description)

    await page.getByRole('button', { name: `About ${name}` }).click()
    await expect(page.getByText(description)).toBeVisible()
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    if (skillId) await api(page, 'removeSkill', { id: skillId }).catch(() => {})
    await page.reload().catch(() => {}) // Closes the skill's details and Edit Profile, which would cover the next spec.
    await browser.close()
  }
})
