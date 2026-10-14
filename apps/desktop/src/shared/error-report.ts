// Error tracking: an uncaught error, as the host's loopback `reportError` takes it. The host
// sends it to PostHog with usage analytics (same switch) after cutting paths down; the
// message never leaves this computer (docs/features/analytics.md).

export interface ErrorReport {
  process: 'desktop-main' | 'desktop-renderer'
  /** The error's class (`TypeError`), or what the platform reports (`RendererGone`). */
  type: string
  /** How it was caught: `onerror`, `unhandledrejection`, `uncaught`, `render-process-gone`, … */
  mechanism: string
  /** Only a reason Electron names (`oom`); never an error's message. */
  value?: string
  /** Oldest call first, the failing one last. */
  frames: ReportFrame[]
}

export interface ReportFrame {
  function: string
  file?: string
  line?: number
  column?: number
}

/** `    at fn (file:line:col)` or `    at file:line:col` in a V8 stack. */
const FRAME = /^\s*at (?:(.+?) \()?(.+?):(\d+):(\d+)\)?$/

export function errorReport(error: unknown, process: ErrorReport['process'], mechanism: string): ErrorReport {
  const type = error instanceof Error ? error.name : 'NonError'
  const stack = error instanceof Error ? (error.stack ?? '') : ''
  return { process, type, mechanism, frames: parseStack(stack) }
}

/** V8 lists the failing call first; the report wants it last. */
export function parseStack(stack: string): ReportFrame[] {
  const frames: ReportFrame[] = []
  for (const line of stack.split('\n')) {
    const m = FRAME.exec(line)
    if (m) frames.push({ function: m[1] ?? '<anonymous>', file: m[2], line: Number(m[3]), column: Number(m[4]) })
  }
  return frames.reverse()
}
