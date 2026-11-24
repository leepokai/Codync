import { test, expect } from '@playwright/test'
import { api, attach, setPref } from './sandbox.ts'

test('the canvas bars fold into one button, and a narrow canvas stacks them and keeps its top bar to icons', async () => {
  const { browser, page } = await attach()
  type Listed = { workflows: { id: string }[] }
  const ids = async () => (await api<Listed>(page, 'workflows', {})).workflows.map((w) => w.id)
  const before = new Set(await ids())
  const chatWidth = await page.evaluate(() => localStorage.getItem('detailsPanelWidth'))
  try {
    await setPref(page, 'workflowToolsFolded', false)
    await page.locator('.nav-rail [data-page="workflows"]').click()
    const list = page.locator('aside.sidebar')
    if (((await list.boundingBox())?.width ?? 0) < 50) await page.getByRole('button', { name: 'Toggle Sidebar', exact: true }).click()
    await page.locator('.sidebar-header').getByRole('button', { name: 'New workflow', exact: true }).click()
    await expect(page.locator('.workflow-zoom')).toBeVisible()

    // Collapse folds the zoom and the tools into the Tools button, which stays after a reload and brings them back.
    await page.getByRole('button', { name: 'Collapse', exact: true }).click()
    await expect(page.locator('.workflow-tools')).toHaveCount(0)
    await expect(page.getByRole('button', { name: 'Zoom in', exact: true })).toHaveCount(0)
    await page.reload()
    await page.locator('.nav-rail [data-page="workflows"]').click()
    await page.getByRole('button', { name: 'Tools', exact: true }).click()
    await expect(page.locator('.workflow-tools')).toBeVisible()
    await expect(page.getByRole('button', { name: 'Zoom in', exact: true })).toBeVisible()

    // A wide chat leaves the canvas its narrowest (420): the bars stack, the minimap clears the tools, and History
    // and Run show only their icons, still named.
    await setPref(page, 'workflowChatOpen', true)
    await setPref(page, 'detailsPanelWidth', 10_000)
    await expect.poll(() => page.locator('.workflows').evaluate((e) => e.getBoundingClientRect().width)).toBeLessThanOrEqual(600)
    await expect(page.locator('.workflow-run-now .workflow-hud-label')).toBeHidden()
    await expect(page.locator('.workflow-history .workflow-hud-label')).toBeHidden()
    await expect(page.getByRole('button', { name: 'Run workflow', exact: true })).toBeVisible()
    await expect(page.getByRole('button', { name: 'History', exact: true })).toBeVisible()
    await page.getByRole('button', { name: 'Minimap', exact: true }).click()
    const tools = (await page.locator('.workflow-tools').boundingBox())!
    const zoom = (await page.locator('.workflow-zoom').boundingBox())!
    const minimap = (await page.locator('.workflow-minimap').boundingBox())!
    expect(tools.y + tools.height).toBeLessThanOrEqual(zoom.y)
    expect(minimap.x >= tools.x + tools.width || minimap.y + minimap.height <= tools.y).toBe(true)
  } finally {
    await setPref(page, 'workflowToolsFolded', false).catch(() => {})
    await setPref(page, 'detailsPanelWidth', chatWidth === null ? 400 : JSON.parse(chatWidth)).catch(() => {})
    for (const id of await ids().catch(() => [])) if (!before.has(id)) await api(page, 'deleteWorkflow', { id }).catch(() => {})
    await browser.close()
  }
})
