import { test, expect, type Page } from '@playwright/test'
import { attach } from './sandbox.ts'

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
      const store = new Proxy(local, { get: (t, p) => (p === 'computer' ? shown : p === 'hostName' ? 'Other Mac' : bound(t, p)) })
      app.computers = [...app.computers, computer]
      app.stores.set(other, store)
    } else {
      app.computers = app.computers.filter((c) => c.id !== other)
      app.stores.delete(other)
    }
    app.changed()
  }, [on, other] as const)
}

test('the Workflows page shows a computer menu before its tabs only with more than one computer, and it switches both ways', async () => {
  const { browser, page } = await attach()
  const start = page.locator('.window-bar-start')
  const errors: string[] = []
  page.on('pageerror', (e) => errors.push(String(e.stack ?? e)))
  page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()) })
  try {
    await page.locator('.nav-rail [data-page="workflows"]').click()
    await expect(page.locator('.workflows-page')).toBeVisible()
    // One computer: no menu.
    await expect(start.locator('.choice-picker')).toHaveCount(0)

    await standIn(page, true)
    const picker = start.locator('.choice-picker')
    await expect(picker).toHaveCount(1)
    const local = (await picker.getAttribute('title')) ?? ''
    expect(local).not.toBe('Other Mac')
    await picker.click()
    await expect(page.getByRole('menuitemradio', { name: 'Other Mac', exact: true })).toBeVisible()
    await page.getByRole('menuitemradio', { name: 'Other Mac', exact: true }).click()
    await expect(start.locator('.choice-picker[title="Other Mac"]')).toBeVisible()
    await expect(page.locator('.workflows-page')).toBeVisible()

    await start.locator('.choice-picker').click()
    await page.getByRole('menuitemradio', { name: local, exact: true }).click()
    await expect(start.locator(`.choice-picker[title="${local}"]`)).toBeVisible()
  } catch (e) {
    throw new Error(`${String(e)}\n${errors.join('\n---\n')}`)
  } finally {
    await standIn(page, false).catch(() => {})
    await expect(start.locator('.choice-picker')).toHaveCount(0).catch(() => {})
    await browser.close()
  }
})
