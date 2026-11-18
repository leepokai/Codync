import { test, expect } from '@playwright/test'
import { addAccount, agents, api, attach } from './sandbox.ts'

// Opens a real agent's own sign-in on this computer: only when the maintainer asks for a real login test
// (docs/guides/test-stack.md#real-login-tests), never part of the suite. Repeated automated sign-ins can get an
// account banned by its provider. login-profiles.spec.ts covers the screens on a stand-in agent every time.
test.skip(() => process.env.CODYNC_E2E_REAL_LOGINS !== '1', 'Runs only with CODYNC_E2E_REAL_LOGINS=1.')

test("Add account opens a real agent's sign-in in a terminal, and leaving it signs nothing in", async () => {
  const { browser, page } = await attach()
  let profileId: string | undefined
  try {
    // Their account login profiles sign in in a terminal (a browser sign-in would open on this computer).
    const agent = (await agents(page)).find((b) => b.installed && ['claude', 'codex'].includes(b.id))
    test.skip(!agent, 'Needs Claude Code or Codex installed on this computer.')
    if (!agent) return
    await addAccount(page, agent, (id) => (profileId = id))
    const added = page.locator('.login-profile', { hasText: 'New login profile' })
    await added.getByRole('button', { name: 'Remove login profile' }).click()
    await expect(added).toHaveCount(0)
    profileId = undefined
  } finally {
    if (profileId) await api(page, 'removeLoginProfile', { loginProfile: profileId }).catch(() => {})
    await page.reload() // Leaves Extensions, which would cover what the next spec clicks.
    await browser.close()
  }
})
