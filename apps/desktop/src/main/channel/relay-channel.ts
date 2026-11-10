import { deviceHello, FrameCodec, unb64url, type DeviceKey } from './crypto.ts'

// One end-to-end encrypted channel to an account computer over the Codync relay (docs/reference/remote-relay.md
// §6–§7), as the iPhone's `ChannelTransport` does it, relay only: the computers the desktop app reaches this way
// are cloud computers, which have no address to dial. Open only while something uses it and someone looks at the
// window (`setAttended`), so a cloud computer can sleep once this app stops looking: a phone connects only in the
// foreground the same way.

export interface ChannelError {
  status: number
  message: string
  /** The computer isn't connected to the relay (asleep, or off). */
  offline?: boolean
}

/** What the channel needs from a WebSocket (the global one in the app, a fake in tests). */
export interface Socket {
  send(text: string): void
  close(code?: number): void
  addEventListener(type: 'open' | 'message' | 'close' | 'error', listener: (event: { data?: unknown; code?: number }) => void): void
}

export interface ChannelSetup {
  computerId: string
  /** The computer's Ed25519 key (b64url), which must sign the handshake. */
  signKey: string
  device: DeviceKey
  /** Opens `GET {cloud}/v1/relay/device/{computerId}?v=1`, signed with the device key. */
  dial: () => Socket
  /** Called once it turned this device away for good, so the next use starts over. */
  onRefused: () => void
}

type Inner = { id: number; ok?: unknown; err?: { status?: number; message?: string }; ev?: unknown; end?: boolean }

interface Call {
  method: string
  body: unknown
  id: number | null
  resolve: (value: unknown) => void
  reject: (error: ChannelError) => void
  timer: ReturnType<typeof setTimeout>
}

interface Sub {
  sub: 'events' | 'term'
  body: unknown
  id: number | null
  onEvent: (event: unknown) => void
  onEnd: (error: ChannelError | null) => void
}

const PING = '{"t":"ping"}'
const HEARTBEAT = 30_000
/** Nothing heard for this long (the relay answers every ping): the socket is dead. */
const SILENCE = 75_000
const HANDSHAKE = 10_000
/** Nothing has used it for this long: let go of it. */
const IDLE = 15_000
/** How this end closes a socket it gave up on. A WebSocket client may send only 1000 or 3000–4999 (1011 throws). */
const BROKEN = 4000

export const unreachable = (): ChannelError => ({ status: 0, message: "Can't reach your computer. Is it on and connected to the internet?" })
const asleep = (): ChannelError => ({ status: 0, message: 'Your computer is offline.', offline: true })

export class RelayChannel {
  state: 'idle' | 'connecting' | 'asleep' | 'ready' | 'refused' = 'idle'
  private socket: Socket | null = null
  private codec: FrameCodec | null = null
  private hello: ReturnType<typeof deviceHello> | null = null
  private nextId = 0
  private calls = new Set<Call>()
  private subs = new Set<Sub>()
  private refusal: ChannelError | null = null
  private attended = false
  private heard = 0
  private backoff = 0
  private retry: ReturnType<typeof setTimeout> | null = null
  private idle: ReturnType<typeof setTimeout> | null = null
  private heartbeat: ReturnType<typeof setInterval> | null = null
  private handshakeTimer: ReturnType<typeof setTimeout> | null = null
  private readonly setup: ChannelSetup

  constructor(setup: ChannelSetup) {
    this.setup = setup
  }

  get signKey() {
    return this.setup.signKey
  }

  /** An inner request (§6.5). Waits for the channel up to `timeoutMs`, a sleeping computer's wake included. */
  call(method: string, body: unknown, timeoutMs: number): Promise<unknown> {
    if (this.refusal) return Promise.reject(this.refusal)
    return new Promise((resolve, reject) => {
      const call: Call = {
        method,
        body,
        id: null,
        resolve,
        reject,
        timer: setTimeout(() => {
          if (!this.calls.delete(call)) return
          if (call.id !== null) this.sendInner({ id: call.id, cancel: true })
          reject(this.state === 'asleep' ? asleep() : unreachable())
          this.settle()
        }, timeoutMs),
      }
      this.calls.add(call)
      this.use()
      if (this.codec) this.sendCall(call)
    })
  }

  /** An `events` or `term` subscription; the returned function stops it. It ends when the channel drops. */
  subscribe(sub: Sub['sub'], body: unknown, onEvent: (event: unknown) => void, onEnd: (error: ChannelError | null) => void): () => void {
    const s: Sub = { sub, body, id: null, onEvent, onEnd }
    if (this.refusal) {
      onEnd(this.refusal)
      return () => {}
    }
    this.subs.add(s)
    this.use()
    if (this.codec) this.sendSub(s)
    return () => {
      if (!this.subs.delete(s)) return
      if (s.id !== null) this.sendInner({ id: s.id, cancel: true })
      this.settle()
    }
  }

  /**
   * Nobody looks at the window: the socket goes, whatever views still ask for (a list they refresh) waits until
   * someone does, up to each call's timeout. Looking again reconnects.
   */
  setAttended(on: boolean) {
    if (on === this.attended) return
    this.attended = on
    if (on) {
      if (this.calls.size || this.subs.size) this.use()
      return
    }
    if (this.retry) clearTimeout(this.retry)
    this.retry = null
    this.socket?.close(1000)
  }

  /** Ends everything (signed out). */
  close() {
    this.refuse({ status: 0, message: 'Signed out.' }, false)
  }

  // MARK: demand

  private use() {
    if (this.idle) clearTimeout(this.idle)
    this.idle = null
    if (this.attended && !this.socket && !this.retry) this.open()
  }

  /** Nothing left to wait for: the socket goes after `IDLE`. */
  private settle() {
    if (this.calls.size || this.subs.size || this.idle || !this.socket) return
    this.idle = setTimeout(() => {
      this.idle = null
      if (this.calls.size || this.subs.size) return
      this.socket?.close(1000)
    }, IDLE)
  }

  // MARK: socket

  private open() {
    let socket: Socket
    try {
      socket = this.setup.dial()
    } catch {
      return this.again()
    }
    this.socket = socket
    this.state = 'connecting'
    this.heard = Date.now()
    socket.addEventListener('message', (e) => {
      if (this.socket !== socket) return
      // A bug here must cost this socket, not the app (an uncaught error in the main process stops it with a dialog).
      try {
        this.receive(String(e.data))
      } catch (error) {
        console.warn('relay channel:', error)
        socket.close(BROKEN)
      }
    })
    socket.addEventListener('close', (e) => this.socket === socket && this.closed(e.code ?? 1006))
    socket.addEventListener('error', () => this.socket === socket && socket.close(BROKEN))
    this.heartbeat = setInterval(() => {
      if (Date.now() - this.heard > SILENCE) socket.close(BROKEN)
      else socket.send(PING)
    }, HEARTBEAT)
  }

  private receive(text: string) {
    this.heard = Date.now()
    let wire: Record<string, unknown>
    try {
      wire = JSON.parse(text) as Record<string, unknown>
    } catch {
      return
    }
    switch (wire.t) {
      case 'presence':
        // The computer (re)joined: whatever channel there was is gone; a fresh handshake.
        this.drop(wire.online === true ? unreachable() : asleep())
        if (wire.online === true) this.startHandshake()
        else this.state = 'asleep'
        break
      case 'welcome':
        return this.welcome(wire as { ek: string; sig: string })
      case 'reject':
        return this.rejected(String(wire.code ?? ''), typeof wire.message === 'string' ? wire.message : null)
      case 'f':
        return this.frame(wire as { c: number; d: string })
    }
  }

  private startHandshake() {
    this.hello = deviceHello(this.setup.device, unb64url(this.setup.signKey))
    this.state = 'connecting'
    this.socket?.send(JSON.stringify(this.hello.hello))
    const socket = this.socket
    if (this.handshakeTimer) clearTimeout(this.handshakeTimer)
    this.handshakeTimer = setTimeout(() => this.hello && this.socket === socket && socket?.close(1000), HANDSHAKE)
  }

  private welcome(wire: { ek: string; sig: string }) {
    const hello = this.hello
    if (!hello) return
    this.hello = null
    try {
      this.codec = new FrameCodec(hello.finish(wire))
    } catch {
      // Signed by another key: not the computer this account knows.
      return this.refuse({ status: 401, message: "This computer's identity changed. Remove it from your account and add it again." })
    }
    this.state = 'ready'
    this.backoff = 0
    for (const call of this.calls) if (call.id === null) this.sendCall(call)
    for (const sub of this.subs) if (sub.id === null) this.sendSub(sub)
  }

  private rejected(code: string, message: string | null) {
    if (code === 'unauthorized' || code === 'revoked') return this.refuse({ status: 401, message: message ?? "This device isn't allowed on that computer anymore." })
    if (code === 'unsupportedVersion') return this.refuse({ status: 426, message: 'Update Codync to keep using this computer.' })
    this.socket?.close(1000)
  }

  private frame(wire: { c: number; d: string }) {
    let inner: Inner | undefined
    try {
      inner = this.codec?.open(wire) as Inner | undefined
    } catch {
      this.socket?.close(4002)
      return
    }
    if (!inner || typeof inner.id !== 'number') return
    for (const call of this.calls) {
      if (call.id !== inner.id) continue
      if (!('ok' in inner) && !inner.err) return
      this.calls.delete(call)
      clearTimeout(call.timer)
      if (inner.err) call.reject({ status: inner.err.status ?? 500, message: inner.err.message ?? `Host error ${inner.err.status ?? 500}` })
      else call.resolve(inner.ok)
      return this.settle()
    }
    for (const sub of this.subs) {
      if (sub.id !== inner.id) continue
      if ('ev' in inner) return sub.onEvent(inner.ev)
      if (inner.end || inner.err) {
        this.subs.delete(sub)
        sub.onEnd(inner.err ? { status: inner.err.status ?? 500, message: inner.err.message ?? 'Host error' } : null)
        this.settle()
      }
      return
    }
  }

  private closed(code: number) {
    this.socket = null
    this.stopTimers()
    if (code === 4001 || code === 4003) return this.refuse({ status: 401, message: "This device isn't allowed on that computer anymore." })
    if (code === 4400) return this.refuse({ status: 426, message: 'Update Codync to keep using this computer.' })
    this.drop(this.state === 'asleep' ? asleep() : unreachable())
    this.state = 'idle'
    if (this.attended && (this.calls.size || this.subs.size)) this.again()
  }

  private stopTimers() {
    if (this.heartbeat) clearInterval(this.heartbeat)
    if (this.handshakeTimer) clearTimeout(this.handshakeTimer)
    if (this.idle) clearTimeout(this.idle)
    this.heartbeat = this.handshakeTimer = this.idle = null
    this.hello = null
  }

  /** Reconnects after a delay: 1 s doubling to 30 s, full jitter (§7.6). */
  private again() {
    const delay = Math.random() * Math.min(30_000, 1000 * 2 ** this.backoff++)
    this.retry = setTimeout(() => {
      this.retry = null
      if (this.attended && !this.socket && (this.calls.size || this.subs.size)) this.open()
    }, delay)
  }

  /** The channel is gone: what was sent on it can't be answered any more. What still waits to be sent waits on. */
  private drop(error: ChannelError) {
    this.codec = null
    for (const call of this.calls) {
      if (call.id === null) continue
      this.calls.delete(call)
      clearTimeout(call.timer)
      call.reject(error)
    }
    for (const sub of this.subs) {
      if (sub.id === null) continue
      this.subs.delete(sub)
      sub.onEnd(error)
    }
  }

  private refuse(error: ChannelError, report = true) {
    this.refusal = error
    this.state = 'refused'
    if (this.retry) clearTimeout(this.retry)
    this.retry = null
    const socket = this.socket
    this.socket = null
    this.stopTimers()
    socket?.close(1000)
    this.codec = null
    for (const call of this.calls) {
      clearTimeout(call.timer)
      call.reject(error)
    }
    for (const sub of this.subs) sub.onEnd(error)
    this.calls.clear()
    this.subs.clear()
    if (report) this.setup.onRefused()
  }

  // MARK: inner RPC

  private sendCall(call: Call) {
    call.id = this.id()
    this.sendInner({ id: call.id, m: call.method, b: call.body ?? null })
  }

  private sendSub(sub: Sub) {
    sub.id = this.id()
    this.sendInner({ id: sub.id, sub: sub.sub, b: sub.body })
  }

  private id() {
    this.nextId = this.nextId >= 0xffff_ffff ? 1 : this.nextId + 1
    return this.nextId
  }

  private sendInner(message: unknown) {
    if (!this.codec || !this.socket) return
    for (const frame of this.codec.seal(message)) this.socket.send(JSON.stringify(frame))
  }
}
