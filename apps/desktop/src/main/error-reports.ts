import { app, ipcMain } from 'electron'
import { errorReport, type ErrorReport } from '../shared/error-report'
import type { HostController } from './host-controller'

/** Not crashes: a helper that exited normally, or was ended from outside (logout, shutdown, an update). */
const ENDED_FROM_OUTSIDE = new Set(['clean-exit', 'killed'])

/**
 * Error tracking: uncaught errors in the main process and the windows, and windows or helpers
 * that crashed, go to the host's loopback `reportError` (shared/error-report.ts).
 */
export function registerErrorReports(host: HostController) {
  const send = (report: ErrorReport) => {
    host.call('reportError', report).catch(() => {})
  }
  // The monitor only watches: Electron still handles the error as before.
  process.on('uncaughtExceptionMonitor', (error) => send(errorReport(error, 'desktop-main', 'uncaught')))
  process.on('unhandledRejection', (reason) => {
    console.error('Unhandled rejection in the main process:', reason)
    send(errorReport(reason, 'desktop-main', 'unhandledrejection'))
  })
  ipcMain.on('app:reportError', (_e, report: ErrorReport) => send({ ...report, process: 'desktop-renderer' }))
  app.on('render-process-gone', (_e, _contents, details) => {
    if (!ENDED_FROM_OUTSIDE.has(details.reason)) send({ process: 'desktop-main', type: 'RendererGone', mechanism: 'render-process-gone', value: details.reason, frames: [] })
  })
  app.on('child-process-gone', (_e, details) => {
    // A GPU process that can't start (Linux without GPU access) only means software rendering: the app goes on.
    const softwareRendering = details.type === 'GPU' && details.reason === 'launch-failed'
    if (!ENDED_FROM_OUTSIDE.has(details.reason) && !softwareRendering) send({ process: 'desktop-main', type: `${details.type}Gone`, mechanism: 'child-process-gone', value: details.reason, frames: [] })
  })
}
