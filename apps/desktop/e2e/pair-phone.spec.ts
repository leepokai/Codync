import { test, expect } from '@playwright/test'
import { api, appPage, attach, openPage, withPrefs } from './sandbox.ts'

type Hello = { name: string }

test('Computers › Pair phone uses device-neutral copy and closes with one Escape', async () => {
  const { browser, page } = await attach()
  let restorePrefs: (() => Promise<void>) | undefined
  try {
    restorePrefs = await withPrefs(page, { appLanguage: 'en' })
    const { name } = await api<Hello>(page, 'hello', {})
    await openPage(page, 'Computers')
    const computers = appPage(page)
    const heading = computers.getByRole('heading', { name: 'Computers & devices', exact: true })
    const localComputer = computers.locator('.card-section').filter({
      has: page
        .locator('.settings-row > .settings-stack > span[style*="var(--text-caption)"][style*="var(--secondary)"]')
        .filter({ hasText: /^This (Mac|computer) · / }),
    })
    await expect(heading).toBeVisible()
    await expect(localComputer).toBeVisible()
    await expect(computers.getByRole('button', { name: 'Pair iPhone', exact: true })).toHaveCount(0)
    await localComputer.getByRole('button', { name: 'Pair phone', exact: true }).click()

    const titlePrefix = 'Pair phone with'
    const sheet = page.getByRole('dialog', { name: `${titlePrefix} ${name}`, exact: true })
    await expect(sheet).toBeVisible()
    await expect(sheet.getByText(/Scan with the Codync app or your phone's camera\./)).toBeVisible()

    const renderedName = name.replace(/[ \t\r\n\f]+/g, ' ')
    const networkPrefix = '“Reach from anywhere” is off, so the phone only reaches'
    const networkSuffix = 'on the same network.'
    const [sheetTitle, ...body] = (await sheet.innerText()).split('\n')
    const networkNoteIndex = body.indexOf(`${networkPrefix} ${renderedName} ${networkSuffix}`)
    if (networkNoteIndex !== -1) body[networkNoteIndex] = `${networkPrefix} ${networkSuffix}`
    const fixedTitle = sheetTitle === `${titlePrefix} ${renderedName}` ? titlePrefix : sheetTitle
    const fixedCopy = [fixedTitle, ...body].join('\n')
    expect(/iPhone/i.test(fixedCopy), 'Pairing sheet copy should be device-neutral').toBe(false)

    await page.keyboard.press('Escape')
    await expect(sheet).toBeHidden()
    await expect(heading).toBeVisible()
  } finally {
    await restorePrefs?.()
    await browser.close()
  }
})
