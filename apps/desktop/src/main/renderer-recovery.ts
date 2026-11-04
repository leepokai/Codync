import { app, type RenderProcessGoneDetails, type WebContents } from 'electron'

/** Why a window's page can die and come back by loading again; `clean-exit` and `killed` mean it was ended on purpose. */
const RECOVERABLE: ReadonlySet<RenderProcessGoneDetails['reason']> = new Set(['crashed', 'oom', 'abnormal-exit', 'memory-eviction'])
/** Reloads in a row: a page that crashes again after these stays down instead of looping. */
const MAX_RELOADS = 2
/** A page that stayed up this long since its reload counts as recovered: its next crash starts the count again. */
const RECOVERED_MS = 60_000

/** A window whose page crashed loads it again instead of staying blank (error-reports.ts reports the crash). */
export function registerRendererRecovery() {
  const reloads = new WeakMap<WebContents, { at: number; count: number }>()
  app.on('render-process-gone', (_e, contents, { reason }) => {
    if (!RECOVERABLE.has(reason) || contents.isDestroyed()) return
    const last = reloads.get(contents)
    const count = last && Date.now() - last.at < RECOVERED_MS ? last.count + 1 : 1
    if (count > MAX_RELOADS) return
    reloads.set(contents, { at: Date.now(), count })
    contents.reload()
  })
}
