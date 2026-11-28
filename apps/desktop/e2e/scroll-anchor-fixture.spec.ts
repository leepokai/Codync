import { test, expect, type Page } from '@playwright/test'
import { resolve } from 'node:path'
import { attach } from './sandbox.ts'

const fixturePath = resolve(__dirname, 'scroll-anchor-fixture.tsx').replace(/\\/g, '/')
const fixture = fixturePath.startsWith('/') ? `/@fs${fixturePath}` : `/@fs/${fixturePath}`

type Handle = {
  unmount: () => void
  calls: () => string[]
  remove: (id: string) => void
  removeMany: (ids: string[]) => void
  regroup: (first: string, second: string) => void
  append: (id: string) => void
  reveal: (entryId: string) => void
}
type FixtureWindow = Window & { __scrollAnchorHandle?: Handle }
type MountOptions = { count?: number; canLoadOlder?: boolean; loadMode?: 'none' | 'one-visible' | 'hold' }

async function mount(page: Page, options: MountOptions) {
  await page.evaluate(async ([path, options]) => {
    const { mount } = (await import(path as string)) as typeof import('./scroll-anchor-fixture.tsx')
    ;(window as FixtureWindow).__scrollAnchorHandle?.unmount()
    ;(window as FixtureWindow).__scrollAnchorHandle = mount(options as Parameters<typeof mount>[0])
  }, [fixture, options] as const)
}

const rows = (page: Page) => page.locator('.e2e-scroll-anchor [data-row-id]')
const calls = (page: Page) => page.evaluate(() => (window as FixtureWindow).__scrollAnchorHandle!.calls())

/** Scrolls the mounted transcript like the reader, then waits for its own scroll handler to settle. */
async function scrollTo(page: Page, top: number) {
  await page.evaluate((top) => {
    document.querySelector<HTMLElement>('.e2e-scroll-anchor .transcript')!.scrollTop = top
  }, top)
  await expect.poll(() => page.evaluate(() => Math.round(document.querySelector<HTMLElement>('.e2e-scroll-anchor .transcript')!.scrollTop))).toBe(top)
  await page.waitForTimeout(100)
}

/** The row at the view's top edge, and how far its top is below that edge. */
const topRow = (page: Page) => page.evaluate(() => {
  const el = document.querySelector<HTMLElement>('.e2e-scroll-anchor .transcript')!
  const viewTop = el.getBoundingClientRect().top
  const row = [...el.querySelectorAll<HTMLElement>('[data-row-id]')].find((r) => r.getBoundingClientRect().bottom > viewTop + 2)
  return row ? { id: row.dataset.rowId!, offset: Math.round(row.getBoundingClientRect().top - viewTop) } : null
})

const offsetOf = (page: Page, id: string) => page.evaluate((id) => {
  const el = document.querySelector<HTMLElement>('.e2e-scroll-anchor .transcript')!
  const row = el.querySelector<HTMLElement>(`[data-row-id="${id}"]`)
  return row ? Math.round(row.getBoundingClientRect().top - el.getBoundingClientRect().top) : null
}, id)

test('a trace-only cache asks once and exposes the older visible message', async () => {
  const { page } = await attach()
  try {
    await mount(page, { count: 0, canLoadOlder: true, loadMode: 'one-visible' })
    await expect(rows(page)).toHaveCount(0)
    await page.locator('.e2e-scroll-anchor .load-earlier').dispatchEvent('click')
    await expect(rows(page)).toHaveCount(1)
    await expect(rows(page).first()).toHaveAttribute('data-row-id', 'older')
    expect(await calls(page)).toEqual(['loadOlder'])

    // A host that has no visible older row is asked once per press, not looped by the same press.
    await mount(page, { count: 0, canLoadOlder: true, loadMode: 'none' })
    await page.locator('.e2e-scroll-anchor .load-earlier').dispatchEvent('click')
    await expect.poll(() => calls(page)).toEqual(['loadOlder'])
    await page.waitForTimeout(300)
    expect(await calls(page)).toEqual(['loadOlder'])
  } finally {
    await page.evaluate(() => (window as FixtureWindow).__scrollAnchorHandle?.unmount()).catch(() => {})
  }
})

test('a fully cached page opens with no host request even while the host is delayed', async () => {
  const { page } = await attach()
  try {
    await mount(page, { count: 80, canLoadOlder: true, loadMode: 'hold' })
    await expect(rows(page)).toHaveCount(40)
    await page.locator('.e2e-scroll-anchor .load-earlier').dispatchEvent('click')
    await expect(rows(page)).toHaveCount(80, { timeout: 2_000 })
    expect(await calls(page), 'the cached page waited for or asked the host').toEqual([])
  } finally {
    await page.evaluate(() => (window as FixtureWindow).__scrollAnchorHandle?.unmount()).catch(() => {})
  }
})

test('revealing the newest card after opening a cached page leaves exactly one page after an arrival', async () => {
  const { page } = await attach()
  try {
    await mount(page, { count: 80, canLoadOlder: true, loadMode: 'hold' })
    await expect(rows(page)).toHaveCount(40)
    // A follower first expands to the fully cached 80-row page. Revealing its newest card must not preserve that
    // expanded window: the next arrival would then make 81 rows.
    await page.locator('.e2e-scroll-anchor .load-earlier').dispatchEvent('click')
    await expect(rows(page)).toHaveCount(80)
    expect(await calls(page)).toEqual([])
    await page.waitForTimeout(150)
    await page.evaluate(() => (window as FixtureWindow).__scrollAnchorHandle!.reveal('e79'))
    await page.waitForTimeout(150)
    await page.evaluate(() => (window as FixtureWindow).__scrollAnchorHandle!.append('e80'))
    await expect(rows(page), 'the reveal pinned the follower to an explicit window').toHaveCount(40)
    await expect.poll(async () => page.evaluate(() => {
      const el = document.querySelector<HTMLElement>('.e2e-scroll-anchor .transcript')!
      return Math.round(el.scrollHeight - el.scrollTop - el.clientHeight)
    })).toBeLessThan(8)
  } finally {
    await page.evaluate(() => (window as FixtureWindow).__scrollAnchorHandle?.unmount()).catch(() => {})
  }
})

test('deleting a rendered row above the viewport leaves the row being read pixel-still', async () => {
  const { page } = await attach()
  try {
    await mount(page, { count: 80 })
    await expect(rows(page)).toHaveCount(40)
    await page.waitForTimeout(300)
    // The reader is well inside the page; e50 is rendered above the viewport but below the window's first row.
    await scrollTo(page, 1200)
    const before = await topRow(page)
    expect(before, 'a row was at the top edge').not.toBeNull()
    // e50 is above the viewport and below the window's first row: deleting it must not move the row being read.
    expect(before!.id.localeCompare('e50', undefined, { numeric: true })).toBeGreaterThan(0)
    await page.evaluate(() => (window as FixtureWindow).__scrollAnchorHandle!.remove('e50'))
    await expect(rows(page)).toHaveCount(39)
    await expect.poll(async () => offsetOf(page, before!.id), { message: 'the surviving row moved down/up with the deletion', timeout: 2_000 })
      .toBe(before!.offset)
  } finally {
    await page.evaluate(() => (window as FixtureWindow).__scrollAnchorHandle?.unmount()).catch(() => {})
  }
})

test('deleting the active paging row falls back to the next row crossing the viewport', async () => {
  const { page } = await attach()
  try {
    // Open the fully cached page first, then position e14 crossing the top edge at -30 px with e15 at +50 px. The
    // second press asks the host and holds its place on e14.
    await mount(page, { count: 80, canLoadOlder: true, loadMode: 'hold' })
    await expect(rows(page)).toHaveCount(40)
    await page.locator('.e2e-scroll-anchor .load-earlier').dispatchEvent('click')
    await expect(rows(page)).toHaveCount(80)
    expect(await calls(page)).toEqual([])
    const atE14 = await page.evaluate(() => {
      const el = document.querySelector<HTMLElement>('.e2e-scroll-anchor .transcript')!
      const row = el.querySelector<HTMLElement>('[data-row-id="e14"]')!
      return Math.round(row.getBoundingClientRect().top - el.getBoundingClientRect().top + el.scrollTop) + 30
    })
    await scrollTo(page, atE14)
    const before = await topRow(page)
    expect(before?.id).toBe('e14')
    expect(before!.offset).toBeLessThan(0)
    const next = await offsetOf(page, 'e15')
    expect(next).not.toBeNull()
    expect(next!).toBeGreaterThanOrEqual(0)
    // The press takes its place on e14, then the active paging row and one row above it are deleted before the held
    // request answers. e15 is the first surviving row the reader can keep.
    await page.locator('.e2e-scroll-anchor .load-earlier').dispatchEvent('click')
    await expect.poll(() => calls(page)).toEqual(['loadOlder'])
    await page.evaluate(() => (window as FixtureWindow).__scrollAnchorHandle!.removeMany(['e13', 'e14']))
    await expect(rows(page)).toHaveCount(78)
    await expect.poll(async () => offsetOf(page, 'e15'), { message: 'the next surviving visible row moved', timeout: 2_000 }).toBe(next)
  } finally {
    await page.evaluate(() => (window as FixtureWindow).__scrollAnchorHandle?.unmount()).catch(() => {})
  }
})

test('a regroup below the viewport does not undo a reader scroll whose event has not arrived', async () => {
  const { page } = await attach()
  try {
    await mount(page, { count: 80 })
    await expect(rows(page)).toHaveCount(40)
    await page.waitForTimeout(300)
    await scrollTo(page, 1200)
    // Move the reader to 1440 and commit a regroup below the viewport in the same JavaScript task, before the browser
    // delivers the scroll event. The saved reading offsets still describe 1200; that must not roll the view back.
    await page.evaluate(() => {
      const el = document.querySelector<HTMLElement>('.e2e-scroll-anchor .transcript')!
      el.scrollTop = 1440
      ;(window as FixtureWindow).__scrollAnchorHandle!.regroup('e70', 'e71')
    })
    await expect(rows(page)).toHaveCount(39)
    await expect.poll(() => page.evaluate(() => Math.round(document.querySelector<HTMLElement>('.e2e-scroll-anchor .transcript')!.scrollTop)))
      .toBe(1440)
  } finally {
    await page.evaluate(() => (window as FixtureWindow).__scrollAnchorHandle?.unmount()).catch(() => {})
  }
})

test('regrouping two rendered rows above the viewport also leaves the row being read pixel-still', async () => {
  const { page } = await attach()
  try {
    await mount(page, { count: 80 })
    await expect(rows(page)).toHaveCount(40)
    await page.waitForTimeout(300)
    await scrollTo(page, 1200)
    const before = await topRow(page)
    expect(before, 'a row was at the top edge').not.toBeNull()
    expect(before!.id.localeCompare('e51', undefined, { numeric: true })).toBeGreaterThan(0)
    // Two earlier rows settle into one grouped row above the viewport, shortening the content by one row.
    await page.evaluate(() => (window as FixtureWindow).__scrollAnchorHandle!.regroup('e50', 'e51'))
    await expect(rows(page)).toHaveCount(39)
    await expect.poll(async () => offsetOf(page, before!.id), { message: 'the surviving row moved when two earlier rows regrouped', timeout: 2_000 })
      .toBe(before!.offset)
  } finally {
    await page.evaluate(() => (window as FixtureWindow).__scrollAnchorHandle?.unmount()).catch(() => {})
  }
})
