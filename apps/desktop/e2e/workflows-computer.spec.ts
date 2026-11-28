import { test, expect, type Page } from '@playwright/test'
import { api, attach } from './sandbox.ts'

type Store = { computer: { id: string }; hostName: string }
type App = { computers: { id: string }[]; stores: Map<string, Store>; local: Store | null; changed(): void }
const other = 'e2e-other-computer'

/** Adds or removes a stand-in second computer: this one under another id and name (one real computer is all a sandbox has). */
async function standIn(page: Page, on: boolean) {
  await page.evaluate(([on, other]) => {
    // The app model, from the React context provider that holds it.
    const root = document.querySelector('#root') as unknown as Record<string, { child: unknown }>
    let fiber = Object.entries(root).find(([k]) => k.startsWith('__reactContainer'))?.[1] as { memoizedProps?: { value?: unknown }; child?: unknown } | undefined
    let app: App | undefined
    const queue = [fiber]
    while (queue.length && !app) {
      fiber = queue.shift()
      const value = fiber?.memoizedProps?.value as App | undefined
      if (value && value.stores instanceof Map && Array.isArray(value.computers)) app = value
      else if (fiber) for (let f = fiber.child as typeof fiber | undefined; f; f = (f as { sibling?: typeof fiber }).sibling) queue.push(f)
    }
    if (!app?.local) throw new Error('No app model with a local computer.')
    const local = app.local
    if (on) {
      const computer = { ...(app.computers.find((c) => c.id === local.computer.id) ?? local.computer), id: other, name: 'Other Mac' }
      // Its methods run on the real store, whose private fields a proxy lacks.
      const bound = (t: Store, p: string | symbol) => { const v = Reflect.get(t, p, t) as unknown; return typeof v === 'function' ? (v as (...a: unknown[]) => unknown).bind(t) : v }
      const shown = { ...local.computer, id: other, name: 'Other Mac' }
      // Its workflows: this one's, and one more that waits for an approval (one array per list this one has).
      const waiting = { id: 'e2e-waiting', title: 'Waiting e2e', revision: 1, enabled: true, updatedAt: 0, running: 'approval' }
      let real: unknown[] = []
      let listed: unknown[] = []
      const workflows = (t: Store) => {
        const now = Reflect.get(t, 'workflows', t) as unknown[]
        if (now !== real) { real = now; listed = [...now, waiting] }
        return listed
      }
      const store = new Proxy(local, { get: (t, p) => (p === 'computer' ? shown : p === 'hostName' ? 'Other Mac' : p === 'workflows' ? workflows(t) : bound(t, p)) })
      app.computers = [...app.computers, computer]
      app.stores.set(other, store)
    } else {
      app.computers = app.computers.filter((c) => c.id !== other)
      app.stores.delete(other)
    }
    app.changed()
  }, [on, other] as const)
}

test('the Workflows page lists each computer\'s workflows beside the canvas, like the Threads list', async () => {
  const { browser, page } = await attach()
  const list = page.locator('aside.sidebar')
  const section = (name: string) => list.getByRole('region', { name, exact: true })
  const errors: string[] = []
  page.on('pageerror', (e) => errors.push(String(e.stack ?? e)))
  type Listed = { workflows: { id: string }[] }
  const ids = async () => (await api<Listed>(page, 'workflows', {})).workflows.map((w) => w.id)
  const before = new Set(await ids())
  try {
    await page.locator('.nav-rail [data-page="workflows"]').click()
    await expect(list.locator('span.sidebar-title')).toHaveText('Workflows')
    const local = (await list.locator('section.thread-group').first().getAttribute('aria-label')) ?? ''
    expect(local).not.toBe('')

    // With one computer, the header's + starts a workflow there; its row is the one shown. Headings have no + of their own.
    await expect(section(local).locator('.thread-group-rows button.thread-row')).toHaveCount(before.size)
    await expect(section(local).getByRole('button', { name: 'New workflow' })).toHaveCount(0)
    await page.locator('.sidebar-header').getByRole('button', { name: 'New workflow', exact: true }).click()
    await expect(section(local).locator('.thread-group-rows button.thread-row')).toHaveCount(before.size + 1)
    await expect(section(local).locator('.thread-group-rows button.thread-row.selected')).toHaveCount(1)
    await expect(page.locator('button.workflow-title[aria-haspopup]')).toBeVisible()
    // It opens with the hand tool.
    await expect(page.getByRole('button', { name: 'Pan the canvas', exact: true })).toHaveClass(/selected/)

    // A computer's heading folds its workflows away and back; the fold stays across a visit to another page.
    await section(local).locator('.thread-section-toggle').click()
    await expect(section(local).locator('.computer-roster-content')).toHaveClass(/collapsed/)
    await page.locator('.nav-rail [data-page="bots"]').click()
    await page.locator('.nav-rail [data-page="workflows"]').click()
    await expect(section(local).locator('.computer-roster-content')).toHaveClass(/collapsed/)
    await section(local).locator('.thread-section-toggle').click()
    await expect(section(local).locator('.computer-roster-content')).not.toHaveClass(/collapsed/)

    // Back on the page after another, nothing new is made and the same workflow shows.
    const title = `Workflow e2e ${Date.now()}`
    const made = (await ids()).find((id) => !before.has(id))!
    await api(page, 'renameWorkflow', { id: made, title })
    await expect(section(local).locator('.thread-group-rows button.thread-row.selected')).toHaveText(title)
    await page.locator('.nav-rail [data-page="bots"]').click()
    await page.locator('.nav-rail [data-page="workflows"]').click()
    await expect(section(local).locator('.thread-group-rows button.thread-row')).toHaveCount(before.size + 1)
    await expect(section(local).locator('.thread-group-rows button.thread-row.selected')).toHaveText(title)

    // Search in the header finds a workflow by its name and opens it.
    if (before.size) await section(local).locator('.thread-group-rows button.thread-row').filter({ hasNotText: title }).first().click()
    await page.locator('.sidebar-header').getByRole('button', { name: 'Search', exact: true }).click()
    await page.getByRole('dialog').getByRole('textbox', { name: 'Search', exact: true }).fill(title)
    await page.keyboard.press('Enter')
    await expect(page.getByRole('dialog')).toHaveCount(0)
    await expect(section(local).locator('.thread-group-rows button.thread-row.selected')).toHaveText(title)

    // A second computer gets its own section; a row there opens on that computer, and back.
    await standIn(page, true)
    await expect(section('Other Mac')).toBeVisible()
    // With several, the header's + asks which computer.
    await page.locator('.sidebar-header').getByRole('button', { name: 'New workflow', exact: true }).click()
    await expect(page.getByRole('menuitem', { name: 'Other Mac' })).toBeVisible()
    await expect(page.getByRole('menuitem', { name: local })).toBeVisible()
    await page.keyboard.press('Escape')
    await expect(page.getByRole('menuitem', { name: 'Other Mac' })).toHaveCount(0)
    // Picking a computer starts the workflow there (the stand-in shares this one's workflows, so the selection tells).
    await page.locator('.sidebar-header').getByRole('button', { name: 'New workflow', exact: true }).click()
    await page.getByRole('menuitem', { name: 'Other Mac' }).click()
    await expect(section('Other Mac').locator('.thread-group-rows button.thread-row.selected')).toHaveCount(1)
    await expect(section(local).locator('.thread-group-rows button.thread-row.selected')).toHaveCount(0)
    // Folded, a computer's heading marks a workflow there that waits for the user; unfolded, its row does.
    const heading = section('Other Mac').locator('.thread-section-toggle')
    await expect(heading.locator('.thread-mark')).toHaveCount(0)
    await heading.click()
    await expect(heading.locator('.thread-mark')).toBeVisible()
    await heading.click()
    await expect(heading.locator('.thread-mark')).toHaveCount(0)
    await section(local).locator('.thread-group-rows button.thread-row').first().click()
    await expect(section(local).locator('.thread-group-rows button.thread-row.selected')).toHaveCount(1)
    await expect(section('Other Mac').locator('.thread-group-rows button.thread-row.selected')).toHaveCount(0)
  } catch (e) {
    throw new Error(`${String(e)}\n${errors.join('\n---\n')}`)
  } finally {
    await standIn(page, false).catch(() => {})
    // The workflows this test made go again.
    for (const id of await ids().catch(() => [])) if (!before.has(id)) await api(page, 'deleteWorkflow', { id }).catch(() => {})
    await browser.close()
  }
})
