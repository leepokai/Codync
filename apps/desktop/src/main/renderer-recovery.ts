import { app } from 'electron'
import { trackBody } from '../shared/analytics'
import { pageRecovery } from '../shared/renderer-recovery'
import type { HostController } from './host-controller'

/** A window whose page crashed loads it again instead of staying blank (error-reports.ts reports the crash). */
export function registerRendererRecovery(host: HostController) {
  const onPageGone = pageRecovery((outcome) => {
    // Best effort: the host may be down or restarting, and the crash itself is reported apart.
    host.call('track', trackBody({ event: 'window_recovery', outcome })).catch(() => {})
  })
  app.on('render-process-gone', (_e, contents, { reason }) => {
    if (!contents.isDestroyed()) onPageGone(contents, reason)
  })
}
