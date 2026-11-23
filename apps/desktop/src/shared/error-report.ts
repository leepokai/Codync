// Error tracking: an uncaught error, as the host's loopback `reportError` takes it. The host
// sends it to PostHog with usage analytics (same switch) after cutting paths down; the
// message never leaves this computer (docs/features/analytics.md).

export interface ErrorReport {
  process: 'desktop-main' | 'desktop-renderer'
  /** The error's class (`TypeError`), or what the platform reports (`RendererGone`). */
  type: string
  /** How it was caught: `onerror`, `unhandledrejection`, `uncaught`, `render-process-gone`, … */
  mechanism: string
  /** Only what Electron itself names: a crash reason, a helper's name and exit code, a failed system call and its result code; never an error's message. */
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

/** Why a window or helper process ended, as Electron gives it. */
export interface GoneDetails {
  reason: string
  exitCode: number
  /** A helper's non-localized name (`network.mojom.NetworkService`), from `child-process-gone`. */
  name?: string
}

/** A signed 32-bit Windows code (NTSTATUS, HRESULT) as PostHog searches it: `0xC0000005`. */
const hex32 = (code: number) => `0x${(code >>> 0).toString(16).toUpperCase()}`

/**
 * `crashed` · the helper · its exit code: a Windows exit code in hex is the NTSTATUS that
 * names the fault (`0xC0000005` an access violation), elsewhere the signal or exit status.
 */
export function goneValue({ reason, exitCode, name }: GoneDetails, platform: string): string {
  const code = platform === 'win32' && exitCode < 0 ? hex32(exitCode) : String(exitCode)
  return [reason, name, `exit ${code}`].filter(Boolean).join(' · ')
}

/**
 * Electron's own text for a toast Windows wouldn't show (`WinAPI: Show failed, ERROR … (HRESULT: -2143420143)`):
 * the step that failed and the HRESULT in hex, so `0x803E0111` (notifications turned off) tells apart from a broken toast.
 * Anything else Electron or the system says is dropped.
 */
export function notificationFailure(error: string): string {
  const hresult = /HRESULT: (-?\d+)/.exec(error)?.[1]
  const step = /^(WinAPI: [A-Za-z ]+? failed|XML: Invalid XML|Creating ScopedHString failed)/.exec(error)?.[1] ?? (error.startsWith('Notification failed') ? 'Toast failed' : 'Other')
  return hresult ? `${step} ${hex32(Number(hresult))}` : step
}
