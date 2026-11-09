import type { WindowRecoveryOutcome } from './analytics.ts'

/** Why a window's page can die and come back by loading again; `clean-exit` and `killed` mean it was ended on purpose. */
const RECOVERABLE: ReadonlySet<string> = new Set(['crashed', 'oom', 'abnormal-exit', 'memory-eviction'])
/** Reloads in a row: a page that crashes again after these stays down instead of looping. */
const MAX_RELOADS = 2
/** A page that stayed up this long since its reload counts as recovered: its next crash starts the count again. */
const RECOVERED_MS = 60_000

/** A window's reloads in a row, and when the last one was. */
export type Reloads = { at: number; count: number }

/** What a page gone for `reason` does: nothing, reload (with its new count), or stay down. */
export function afterPageGone(reason: string, last: Reloads | undefined, now: number): Reloads | 'stay_down' | null {
  if (!RECOVERABLE.has(reason)) return null
  const count = last && now - last.at < RECOVERED_MS ? last.count + 1 : 1
  return count > MAX_RELOADS ? 'stay_down' : { at: now, count }
}

/** The part of a window's web contents recovery uses. */
type Page = { reload(): void; once(event: 'did-finish-load', listener: () => void): unknown }

/** What to do with a page gone for a reason: reload it or let it stay down, reporting one outcome per recovery. */
export function pageRecovery(report: (outcome: WindowRecoveryOutcome) => void) {
  const reloads = new WeakMap<Page, Reloads>()
  const loading = new WeakSet<Page>()
  return (page: Page, reason: string) => {
    const next = afterPageGone(reason, reloads.get(page), Date.now())
    if (next === null) return
    if (next === 'stay_down') {
      report('gave_up')
      return
    }
    reloads.set(page, next)
    // A reload that crashes before it loads leaves this waiting for the next one.
    if (!loading.has(page)) {
      loading.add(page)
      page.once('did-finish-load', () => {
        loading.delete(page)
        report('reloaded')
      })
    }
    page.reload()
  }
}
