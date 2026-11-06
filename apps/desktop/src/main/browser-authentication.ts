import * as electron from 'electron'

/** A codync://marketplace/<section>/<id> link: the Extensions section and entry to open. */
function codyncLink(url: string): { section: 'apps' | 'agents' | 'skills'; id: string } | null {
  let parsed: URL
  try {
    parsed = new URL(url)
  } catch {
    return null
  }
  const scheme = parsed.protocol.replace(':', '')
  if ((scheme !== 'codync' && scheme !== 'codync-dev') || parsed.hostname !== 'marketplace') return null
  const [section, id] = parsed.pathname.split('/').filter(Boolean)
  if (!id || (section !== 'apps' && section !== 'agents' && section !== 'skills')) return null
  return { section, id }
}

/** A browser may disappear without a callback; its wait must never own the app's loading state. */
export class AuthenticationEnded extends Error {
  readonly reason: 'cancelled' | 'timeout'

  constructor(reason: 'cancelled' | 'timeout', message: string) {
    super(message)
    this.reason = reason
  }
}

interface Pending {
  scheme: string
  accept: (url: string, isCurrent: () => boolean) => Promise<boolean>
  resolve: (url: string) => void
  reject: (error: unknown) => void
  timer: ReturnType<typeof setTimeout>
  callbacks: Promise<void>
}

/** Owns one browser flow, including validation before a callback can consume it. */
export class BrowserAuthentication {
  private pending: Pending | null = null

  private readonly open: (url: string) => Promise<void>
  private readonly timeoutMs: number

  constructor(open: (url: string) => Promise<void>, timeoutMs = 10 * 60_000) {
    this.open = open
    this.timeoutMs = timeoutMs
  }

  authenticate(url: string, scheme: string, accept: (url: string, isCurrent: () => boolean) => Promise<boolean> = async () => true): Promise<string> {
    if (!/^https:\/\//.test(url)) return Promise.reject(new Error('Sign-in needs an https address.'))
    if (this.pending) this.finish(this.pending, new AuthenticationEnded('cancelled', 'Another sign-in started.'))
    return new Promise((resolve, reject) => {
      const pending: Pending = {
        scheme, accept, resolve, reject, callbacks: Promise.resolve(),
        timer: setTimeout(() => this.finish(pending, new AuthenticationEnded('timeout', 'Sign-in timed out.')), this.timeoutMs),
      }
      this.pending = pending
      void Promise.resolve().then(() => this.open(url)).catch((error: unknown) => this.finish(pending, error))
    })
  }

  handleURL(url: string) {
    const link = codyncLink(url)
    if (link) {
      for (const window of electron.BrowserWindow?.getAllWindows?.() ?? []) {
        if (window.isMinimized()) window.restore()
        window.show()
        window.focus()
        window.webContents.send('app:command', { kind: 'openMarketplace', ...link })
      }
      return
    }
    const pending = this.pending
    if (!pending || url.split(':')[0]?.toLowerCase() !== pending.scheme.toLowerCase()) return
    // A stale tab may return before the current one. Validate in order; a rejected callback
    // leaves the current wait intact and cannot race another nonce exchange.
    pending.callbacks = pending.callbacks.then(async () => {
      if (this.pending !== pending) return
      try {
        if (await pending.accept(url, () => this.pending === pending)) this.finish(pending, undefined, url)
      } catch (error) {
        this.finish(pending, error)
      }
    })
  }

  private finish(pending: Pending, error?: unknown, url?: string) {
    if (this.pending !== pending) return
    clearTimeout(pending.timer)
    this.pending = null
    if (url !== undefined) pending.resolve(url)
    else pending.reject(error)
  }
}
