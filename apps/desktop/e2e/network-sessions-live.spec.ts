import { test, expect } from '@playwright/test'
import { spawn } from 'node:child_process'
import { api, appPage, attach, openPage } from './sandbox.ts'

// A session whose MCP server is a process of this test's own: the open page follows it when it ends.
test('a Network page left open sees a coding agent stop running', async () => {
  test.setTimeout(90_000)
  const server = spawn('sleep', ['600'], { stdio: 'ignore' })
  const { browser, page } = await attach()
  try {
    const agent = { kind: 'codex', session: `live-${Date.now()}`, process: `${server.pid}-live`, cwd: '/work/live' }
    const me = await api<{ address: string }>(page, 'agentCall', { agent, name: 'whoami', arguments: {} })
    await openPage(page, 'Network')
    const row = appPage(page).locator('.settings-row', { hasText: me.address })
    await expect(row.getByRole('img', { name: 'Online' })).toBeVisible()
    server.kill()
    // No reload, no new networkView: the host looks again on its own (every 10 s).
    await expect(row.getByRole('img', { name: 'Offline' })).toBeVisible({ timeout: 30_000 })
  } finally {
    server.kill()
    await openPage(page, 'Bots').catch(() => {})
    await browser.close()
  }
})
