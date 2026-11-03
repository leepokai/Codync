import { ipcMain } from 'electron'
import type { AccountComputer, CallError } from '../shared/ipc'
import { computerId, FrameOpener, FrameSealer, Handshake, type DeviceKey } from './channel-crypto'

/**
 * The account's other computers, reached as the phones reach them: the end-to-end encrypted channel
 * (host `remote::channel`) through the account's relay, signed with this install's device key. One
 * channel per computer, opened on first use; a dropped one is opened again by the next call.
 */

type Wire = { t?: string; online?: boolean; ek?: string; sig?: string; code?: string; message?: string; c?: number; d?: string }
type Inner = { id: number; ok?: unknown; err?: { status?: number; message?: string }; ev?: unknown; end?: boolean }
type Pending = { resolve: (value: unknown) => void; reject: (error: CallError) => void; timer: NodeJS.Timeout }
type Subscription = { onEvent: (event: unknown) => void; onEnd: (error: CallError | null) => void }

const unreachable: CallError = { status: 0, message: "Can't reach that computer. Is it on and connected to the internet?" }

/** `Promise.withResolvers`, which the app's ES2023 typing doesn't declare yet. */
function gate() {
  let resolve!: () => void
  let reject!: (error: CallError) => void
  const promise = new Promise<void>((res, rej) => ((resolve = res), (reject = rej)))
  return { promise, resolve, reject }
}

class AccountChannel {
  private socket: WebSocket
  private handshake: Handshake | null = null
  private sealer: FrameSealer | null = null
  private opener: FrameOpener | null = null
  private ready = gate()
  private deadline = setTimeout(() => this.fail({ status: 0, message: "The computer didn't finish connecting." }), 15_000)
  private ids = 0
  private calls = new Map<number, Pending>()
  private subscriptions = new Map<number, Subscription>()
  private pings: NodeJS.Timeout
  failure: CallError | null = null

  constructor(url: string, signature: string, private readonly signKey: string, private readonly device: DeviceKey) {
    this.ready.promise.catch(() => {})
    // Node's WebSocket (undici) takes request headers; the DOM typing the app compiles against doesn't know them.
    const init = { headers: { 'Codync-Sig': signature } } as unknown as string[]
    this.socket = new WebSocket(url, init)
    this.socket.onmessage = (e) => {
      try {
        this.receive(JSON.parse(String(e.data)) as Wire)
      } catch (error) {
        this.fail({ status: 0, message: error instanceof Error ? error.message : String(error) })
      }
    }
    this.socket.onclose = (e) => this.fail(closeError(e.code, e.reason))
    this.socket.onerror = () => this.fail(unreachable)
    // The relay answers these exact bytes itself; they keep the link alive while the chat is quiet.
    this.pings = setInterval(() => this.socket.readyState === WebSocket.OPEN && this.socket.send('{"t":"ping"}'), 30_000)
  }

  private receive(wire: Wire) {
    switch (wire.t) {
      case 'presence':
        if (!wire.online) this.fail({ status: 0, message: 'The computer is offline.' })
        else if (!this.handshake && !this.sealer) {
          this.handshake = new Handshake(this.signKey, this.device)
          this.socket.send(JSON.stringify(this.handshake.hello()))
        } else this.fail({ status: 0, message: 'The computer restarted. Connecting again…' })
        return
      case 'welcome': {
        if (!this.handshake || !wire.ek || !wire.sig) throw new Error('Unexpected host welcome')
        const keys = this.handshake.finish(wire.ek, wire.sig)
        this.handshake = null
        this.sealer = new FrameSealer(keys.d2h)
        this.opener = new FrameOpener(keys.h2d)
        clearTimeout(this.deadline)
        this.ready.resolve()
        return
      }
      case 'reject':
        return this.fail(wire.code === 'leaseExpired'
          ? { status: 0, message: wire.message ?? "The computer can't confirm this device's access right now. Try again shortly." }
          : wire.code === 'unsupportedVersion' ? { status: 426, message: 'Update Codync to connect to this computer.' } : refused(wire.message))
      case 'f': {
        if (!this.opener || wire.c === undefined || !wire.d) throw new Error('Encrypted frame arrived before the handshake')
        const message = this.opener.open(wire.c, wire.d)
        if (message) this.deliver(JSON.parse(message.toString('utf8')) as Inner)
        return
      }
    }
  }

  private deliver(inner: Inner) {
    const sub = this.subscriptions.get(inner.id)
    if ('ev' in inner) return sub?.onEvent(inner.ev)
    if (inner.end) {
      this.subscriptions.delete(inner.id)
      return sub?.onEnd(null)
    }
    const call = this.calls.get(inner.id)
    this.calls.delete(inner.id)
    this.subscriptions.delete(inner.id)
    if (call) clearTimeout(call.timer)
    if (inner.err) {
      const error = { status: inner.err.status ?? 500, message: inner.err.message ?? 'The computer could not complete the request.' }
      call?.reject(error)
      sub?.onEnd(error)
    } else call?.resolve(inner.ok)
  }

  private async send(message: Record<string, unknown>) {
    await this.ready.promise
    if (!this.sealer || this.failure) throw this.failure ?? unreachable
    for (const frame of this.sealer.seal(Buffer.from(JSON.stringify(message)))) this.socket.send(JSON.stringify({ t: 'f', ...frame }))
  }

  call(method: string, body: unknown, timeoutMs: number): Promise<unknown> {
    const id = ++this.ids
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.calls.delete(id)
        void this.send({ id, cancel: true }).catch(() => {})
        reject({ status: 0, message: "The computer didn't respond in time. Check the result before retrying." })
      }, timeoutMs)
      this.calls.set(id, { resolve, reject, timer })
      this.send({ id, m: method, b: body }).catch((error: CallError) => {
        clearTimeout(timer)
        this.calls.delete(id)
        reject(error)
      })
    })
  }

  subscribe(kind: string, body: unknown, subscription: Subscription) {
    const id = ++this.ids
    this.subscriptions.set(id, subscription)
    this.send({ id, sub: kind, b: body }).catch((error: CallError) => {
      if (this.subscriptions.delete(id)) subscription.onEnd(error)
    })
    return () => {
      if (this.subscriptions.delete(id)) void this.send({ id, cancel: true }).catch(() => {})
    }
  }

  fail(error: CallError) {
    if (this.failure) return
    this.failure = error
    clearInterval(this.pings)
    clearTimeout(this.deadline)
    this.ready.reject(error)
    this.socket.close()
    for (const call of this.calls.values()) {
      clearTimeout(call.timer)
      call.reject(error)
    }
    for (const sub of this.subscriptions.values()) sub.onEnd(error)
    this.calls.clear()
    this.subscriptions.clear()
  }
}

/** The computer doesn't let this device in (revoked, never approved): a 401, so callers stop retrying. */
function refused(message?: string): CallError {
  return { status: 401, message: message || "This computer doesn't let this device in. Ask it for access again." }
}

/** What the host's close code means for the person (host `RejectCode::close_code`). */
function closeError(code: number, reason: string): CallError {
  if (code === 4001 || code === 4003 || code === 4410) return refused(reason)
  if (code === 4400) return { status: 426, message: 'Update Codync to connect to this computer.' }
  return unreachable
}

export interface RelaySigner {
  /** The account's relay (its cloud URL), or null when signed out. */
  relay(): string | null
  device(): Promise<DeviceKey>
  /** `Codync-Sig` for a GET of `url`. */
  sign(url: URL): Promise<string>
}

/** Per computer: its channel, or the one being opened, so concurrent first calls share one socket. */
const channels = new Map<string, Promise<AccountChannel>>()

async function open(signer: RelaySigner, computer: AccountComputer): Promise<AccountChannel> {
  const base = signer.relay()
  if (!base) throw { status: 401, message: 'Sign in again to reach your account.' } satisfies CallError
  const url = new URL(`v1/relay/device/${computer.id}?v=1`, `${base}/`)
  const [device, signature] = await Promise.all([signer.device(), signer.sign(url)])
  return new AccountChannel(url.href.replace(/^http/, 'ws'), signature, computer.signKey, device)
}

async function channel(signer: RelaySigner, computer: AccountComputer): Promise<AccountChannel> {
  if (computerId(computer.signKey) !== computer.id) throw { status: 403, message: "This computer's key doesn't match its ID." } satisfies CallError
  const cached = channels.get(computer.id)
  if (cached && !(await cached.then((c) => c.failure, () => true))) return cached
  if (channels.get(computer.id) !== cached) return channel(signer, computer)
  const opening = open(signer, computer)
  channels.set(computer.id, opening)
  return opening
}

/** Forgets `computerId`'s channel and closes it, including one still opening. */
function drop(computerId: string, error: CallError) {
  void channels.get(computerId)?.then((c) => c.fail(error), () => {})
  channels.delete(computerId)
}

/** Signing out or into another account: every channel of the old one closes. */
export function closeAccountChannels() {
  for (const id of [...channels.keys()]) drop(id, { status: 401, message: 'Signed out.' })
}

export function registerAccountChannels(signer: RelaySigner) {
  ipcMain.handle('remote:call', (_e, computer: AccountComputer, method: string, body: unknown, timeoutMs: number) =>
    channel(signer, computer)
      .then((c) => c.call(method, body, timeoutMs))
      .then(
        (value) => ({ ok: true, value }),
        (error: CallError) => ({ ok: false, error }),
      ),
  )
  /** Each open stream's cancel; registered at once, so a close that comes while the channel opens still counts. */
  const streams = new Map<string, () => void>()
  ipcMain.on('remote:stream:open', (e, id: string, computer: AccountComputer, kind: string, body: unknown) => {
    const send = (channelName: string, payload: unknown) => {
      if (!e.sender.isDestroyed()) e.sender.send(channelName, id, payload)
    }
    const onEnd = (error: CallError | null) => {
      streams.delete(id)
      send('host:stream:end', error)
    }
    const pending = () => {}
    streams.set(id, pending)
    channel(signer, computer).then((c) => {
      if (streams.get(id) !== pending) return
      streams.set(id, c.subscribe(kind, body, { onEvent: (event) => send('host:stream:data', JSON.stringify(event)), onEnd }))
    }, (error: CallError) => streams.get(id) === pending && onEnd(error))
  })
  ipcMain.on('remote:stream:close', (_e, id: string) => {
    streams.get(id)?.()
    streams.delete(id)
  })
  ipcMain.on('remote:close', (_e, computerId: string) => drop(computerId, { status: 0, message: 'Closed.' }))
}
