import { appendFile, mkdir, rename, stat, unlink } from 'node:fs/promises'
import { dirname } from 'node:path'
import type { ErrorReport } from '../shared/error-report'

type WindowEvent = 'diagnostics-started' | 'web-contents-created' | 'dom-ready' | 'did-finish-load'
  | 'did-fail-load' | 'unresponsive' | 'responsive' | 'render-process-gone' | 'child-process-gone'

interface WindowDetails {
  webContentsId?: number
  rendererPid?: number
  exitCode?: number
  errorCode?: number
  isMainFrame?: boolean
  reason?: string
  processType?: string
  version?: string
}

const ERROR_TYPES = new Set(['Error', 'TypeError', 'RangeError', 'ReferenceError', 'SyntaxError', 'URIError', 'EvalError', 'AggregateError', 'NonError', 'RendererGone', 'GPUGone', 'UtilityGone'])
const MECHANISMS = new Set(['onerror', 'unhandledrejection', 'uncaught', 'render-process-gone', 'child-process-gone'])
const REASONS = new Set(['clean-exit', 'abnormal-exit', 'killed', 'crashed', 'oom', 'launch-failed', 'integrity-failure', 'memory-eviction'])
const PROCESS_TYPES = new Set(['Browser', 'Tab', 'Utility', 'Zygote', 'Sandbox helper', 'GPU', 'Pepper Plugin', 'Pepper Plugin Broker', 'Unknown'])
const EVENTS = new Set<WindowEvent>(['diagnostics-started', 'web-contents-created', 'dom-ready', 'did-finish-load', 'did-fail-load', 'unresponsive', 'responsive', 'render-process-gone', 'child-process-gone'])

function integer(value: unknown, signed = false): number | undefined {
  return typeof value === 'number' && Number.isInteger(value) && value >= (signed ? -2147483648 : 0) && value <= 2147483647 ? value : undefined
}

function named(value: unknown, allowed: ReadonlySet<string>): string | undefined {
  return typeof value === 'string' && allowed.has(value) ? value : undefined
}

function missing(error: unknown): boolean {
  return (error as NodeJS.ErrnoException | undefined)?.code === 'ENOENT'
}

/** Main-process-only, best effort diagnostics. Never store messages, function names, URLs or full paths. */
export function createClientDiagnostics(file: () => string, options: { maxBytes?: number; maxQueued?: number } = {}) {
  const maxBytes = options.maxBytes ?? 512 * 1024
  const maxQueued = options.maxQueued ?? 128
  const queue: string[] = []
  let writing: Promise<void> | undefined
  let initialized = false
  let size = 0
  let dropped = 0

  async function drain() {
    while (queue.length) {
      const line = queue.shift()!
      try {
        const target = file()
        if (!initialized) {
          await mkdir(dirname(target), { recursive: true })
          try { size = (await stat(target)).size } catch (error) { if (!missing(error)) throw error; size = 0 }
          initialized = true
        }
        const countedDrops = dropped
        const data = JSON.stringify({ ...JSON.parse(line), droppedSinceLastWrite: countedDrops }) + '\n'
        const bytes = Buffer.byteLength(data)
        if (bytes > maxBytes) { dropped++; continue }
        if (size + bytes > maxBytes) {
          try { await unlink(target + '.1') } catch (error) { if (!missing(error)) throw error }
          try { await rename(target, target + '.1') } catch (error) { if (!missing(error)) throw error }
          size = 0
        }
        await appendFile(target, data, { encoding: 'utf8', mode: 0o600 })
        size += bytes
        dropped -= countedDrops
      } catch {
        // A full/unwritable disk must not break IPC, selection, remote reporting or recovery.
        dropped++
        initialized = false
      }
    }
  }

  function write(record: object) {
    if (queue.length >= maxQueued) { dropped++; return }
    const line = JSON.stringify({ at: new Date().toISOString(), pid: process.pid, ...record })
    if (Buffer.byteLength(line) > 4000) { dropped++; return }
    queue.push(line)
    start()
  }

  function start() {
    if (!writing && queue.length) writing = drain().finally(() => { writing = undefined; start() })
  }

  return {
    recordError(report: ErrorReport) {
      try {
      write({
        event: 'error',
        process: report.process === 'desktop-renderer' ? 'desktop-renderer' : 'desktop-main',
        type: named(report.type, ERROR_TYPES) ?? 'OtherError',
        mechanism: named(report.mechanism, MECHANISMS) ?? 'other',
        reason: named(report.value, REASONS),
        frames: (Array.isArray(report.frames) ? report.frames.slice(-16) : []).map((frame) => {
          const base = typeof frame.file === 'string' && frame.file.length <= 2048 ? frame.file.split(/[\\/]/).at(-1) : undefined
          return {
            file: base && /^(?:index(?:-[A-Za-z0-9_-]{6,32})?|main|preload)\.(?:c|m)?js$/.test(base) ? base : 'other',
            line: integer(frame.line), column: integer(frame.column),
          }
        }),
      })
      } catch { dropped++ }
    },
    recordWindow(event: WindowEvent, details: WindowDetails = {}) {
      if (!EVENTS.has(event)) return
      write({
        event,
        webContentsId: integer(details.webContentsId), rendererPid: integer(details.rendererPid),
        exitCode: integer(details.exitCode, true), errorCode: integer(details.errorCode, true),
        isMainFrame: typeof details.isMainFrame === 'boolean' ? details.isMainFrame : undefined,
        reason: named(details.reason, REASONS), processType: named(details.processType, PROCESS_TYPES),
        version: typeof details.version === 'string' && /^\d+\.\d+\.\d+(?:-[A-Za-z0-9.-]{1,24})?$/.test(details.version) ? details.version : undefined,
      })
    },
    // For isolated tests; the application does not wait for this during interaction or shutdown.
    flush: async () => { while (writing) await writing },
  }
}
