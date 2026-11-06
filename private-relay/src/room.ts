/**
 * One in-memory room per computer: the host socket, device sockets, presence and forwarding.
 * ACL and mailbox live in SQLite so they survive a restart. This process never decrypts a frame.
 */

import type { WebSocket } from 'ws'
import { fromB64url, randomId, utf8, verifyEd25519 } from './codec.ts'
import { parseAcl, type Acl } from './acl.ts'
import { ackMailbox, cancelMailbox, deliverNext, expireMailbox, listMailbox, putMailbox, type MailboxNote } from './mailbox.ts'
import type { RelayStore } from './store.ts'

const MAX_DEVICE_SOCKETS = 16
const MAX_PAIRING_SOCKETS = 2
const PAIRING_TTL_MS = 120_000
const PAIRING_MAX_MESSAGES = 16
const RATE_WINDOW_MS = 10_000
const RATE_MAX_MESSAGES = 200
const RATE_MAX_BYTES = 8 * 1024 * 1024
const SILENT_TIMEOUT_MS = 90_000

export const PING = '{"t":"ping"}'
export const PONG = '{"t":"pong"}'

interface HostConn {
  ws: WebSocket
  kid: string
  ready: boolean
  heard: number
  left: boolean
}

interface DeviceConn {
  ws: WebSocket
  dk: string
  link: string
  pair: boolean
  offer?: string
  /** Pairing sockets close when `now` reaches this (offer expiry or the 120s window). */
  until: number
  heard: number
  msgs: number
  /** Traffic in the current `RATE_WINDOW_MS`. */
  rate: { start: number; msgs: number; bytes: number }
  left: boolean
}

interface Room {
  id: string
  host: HostConn | null
  devices: DeviceConn[]
  lastSeenAt: number | null
}

export interface Admit {
  ok: true
  pair: boolean
  offer?: string
}

export interface Refuse {
  ok: false
  status: number
  code: string
}

type Msg = Record<string, unknown> & { t?: unknown }

/** A JSON object. `null` and other non-objects are refused before any field is read. */
function readMessage(raw: string): Msg | null {
  try {
    const value: unknown = JSON.parse(raw)
    if (!value || typeof value !== 'object' || Array.isArray(value)) return null
    return value as Msg
  } catch {
    return null
  }
}

function send(ws: WebSocket | undefined, text: string): void {
  if (!ws || ws.readyState !== ws.OPEN) return
  try {
    ws.send(text)
  } catch {
    // Closed underneath us. The close handler does the bookkeeping.
  }
}

const validClose = (c: unknown): c is number => typeof c === 'number' && Number.isInteger(c) && (c === 1000 || (c >= 3000 && c <= 4999))

export class Rooms {
  private readonly rooms = new Map<string, Room>()
  private readonly store: RelayStore
  private readonly now: () => number

  constructor(store: RelayStore, now: () => number) {
    this.store = store
    this.now = now
  }

  /** Device admission before the socket is accepted. A bad id or pair never creates a computer. */
  admitDevice(computerId: string, dk: string, pair: string): Admit | Refuse {
    const computer = this.store.computer(computerId)
    if (!computer || computer.status !== 'active') return { ok: false, status: 404, code: 'unknownComputer' }
    const acl = this.readAcl(computerId)
    if (!acl) return { ok: false, status: 404, code: 'unknownComputer' }
    const now = this.now()
    const room = this.rooms.get(computerId)
    const devices = room?.devices ?? []
    if (devices.length >= MAX_DEVICE_SOCKETS) return { ok: false, status: 429, code: 'rateLimited' }
    if (pair) {
      const offer = acl.offers.find((o) => o.id === pair && o.exp > now)
      if (!offer) return { ok: false, status: 403, code: 'forbidden' }
      if (devices.filter((d) => d.pair).length >= MAX_PAIRING_SOCKETS) return { ok: false, status: 429, code: 'rateLimited' }
      return { ok: true, pair: true, offer: pair }
    }
    if (!acl.devices.some((d) => d.dk === dk)) return { ok: false, status: 403, code: 'forbidden' }
    return { ok: true, pair: false }
  }

  admitHost(computerId: string, kid: string): { ok: true } | Refuse {
    const computer = this.store.computer(computerId)
    if (!computer) return { ok: false, status: 404, code: 'unknownComputer' }
    if (computer.status !== 'active' || computer.signPub !== kid) return { ok: false, status: 403, code: 'forbidden' }
    return { ok: true }
  }

  attachHost(computerId: string, kid: string, ws: WebSocket): void {
    const room = this.room(computerId)
    const previous = room.host
    if (previous && !previous.left) this.dropHost(room, previous, 4009, 'replaced', true)
    const host: HostConn = { ws, kid, ready: false, heard: this.now(), left: false }
    room.host = host
    ws.on('message', (data, isBinary) => {
      host.heard = this.now()
      if (isBinary) return
      const raw = data.toString()
      if (raw === PING) return send(ws, PONG)
      void this.onHost(room, host, raw).catch(() => {
        // A thrown frame must not reject outside this socket.
        this.dropHost(room, host, 4002, 'protocolError')
      })
    })
    ws.on('close', () => this.dropHost(room, host, 1000, ''))
    ws.on('error', () => this.dropHost(room, host, 1000, ''))
  }

  attachDevice(computerId: string, dk: string, admitted: Admit, ws: WebSocket): void {
    const room = this.room(computerId)
    if (room.devices.length >= MAX_DEVICE_SOCKETS) {
      ws.close(4008, 'rateLimited')
      return
    }
    const now = this.now()
    const device: DeviceConn = {
      ws,
      dk,
      link: randomId(12),
      pair: admitted.pair,
      offer: admitted.offer,
      until: admitted.pair ? this.pairingDeadline(computerId, admitted.offer, now) : Number.POSITIVE_INFINITY,
      heard: now,
      msgs: 0,
      rate: { start: now, msgs: 0, bytes: 0 },
      left: false,
    }
    room.devices.push(device)
    send(ws, JSON.stringify(this.presence(room)))
    const host = room.host
    if (host?.ready) send(host.ws, JSON.stringify({ t: 'open', link: device.link, dk, pair: device.pair }))
    ws.on('message', (data, isBinary) => {
      try {
        device.heard = this.now()
        if (isBinary) return this.dropDevice(room, device, 4002, 'protocolError')
        this.onDevice(room, device, data.toString())
      } catch {
        // One bad frame stays on this socket. The room and every other computer keep running.
        this.dropDevice(room, device, 4002, 'protocolError')
      }
    })
    ws.on('close', () => this.dropDevice(room, device))
    ws.on('error', () => this.dropDevice(room, device))
  }

  /** In-memory rooms. An empty one is dropped, so a disconnect does not keep it. */
  retained(): number {
    return this.rooms.size
  }

  /** Closes silent sockets and drops expired mailbox items, including rows whose room is gone. */
  sweep(): void {
    const now = this.now()
    for (const room of [...this.rooms.values()]) {
      if (room.host && now - room.host.heard > SILENT_TIMEOUT_MS) this.dropHost(room, room.host, 1011, 'timeout')
      for (const device of [...room.devices]) {
        if (device.pair && now >= device.until) this.dropDevice(room, device, 4410, 'pairingExpired')
        else if (now - device.heard > SILENT_TIMEOUT_MS) this.dropDevice(room, device, 1011, 'timeout')
      }
      this.note(room, expireMailbox(this.store, room.id, now))
    }
    // A phone can queue mail and leave while the host is offline. That room is no longer in memory.
    this.store.deleteExpiredAnywhere(now)
  }

  private room(id: string): Room {
    let room = this.rooms.get(id)
    if (!room) {
      room = { id, host: null, devices: [], lastSeenAt: null }
      this.rooms.set(id, room)
    }
    return room
  }

  private readAcl(computerId: string): Acl | null {
    const row = this.store.acl(computerId)
    if (!row) return null
    const bytes = fromB64url(row.body)
    return bytes ? parseAcl(bytes) : null
  }

  private presence(room: Room): { t: 'presence'; online: boolean; lastSeenAt: number | null } {
    return { t: 'presence', online: room.host?.ready === true, lastSeenAt: room.lastSeenAt }
  }

  private pairingDeadline(computerId: string, offer: string | undefined, now: number): number {
    const acl = this.readAcl(computerId)
    const exp = acl?.offers.find((o) => o.id === offer)?.exp ?? now
    return Math.min(now + PAIRING_TTL_MS, exp)
  }

  private async onHost(room: Room, host: HostConn, raw: string): Promise<void> {
    if (host.left || room.host !== host) return
    const msg = readMessage(raw)
    if (!msg) return
    if (msg.t === 'ping') return send(host.ws, PONG)
    if (msg.t === 'acl') return this.onAcl(room, host, msg)
    if (msg.t === 'ready') return this.onReady(room, host)
    if (msg.t === 'data') return this.onData(room, host, msg)
    if (msg.t === 'close') return this.onCloseDevice(room, host, msg)
    if (msg.t === 'mbox.ack') return this.onAck(room, host, msg)
    if (msg.t === 'hooks') return send(host.ws, JSON.stringify({ t: 'hooks.ok' }))
  }

  private async onAcl(room: Room, host: HostConn, msg: Msg): Promise<void> {
    const d = fromB64url(msg.d)
    const sig = fromB64url(msg.sig, 64)
    const pub = fromB64url(host.kid, 32)
    const acl = d && sig && pub && (await verifyEd25519(pub, sig, d)) ? parseAcl(d) : null
    const current = this.store.acl(room.id)?.ver ?? 0
    if (!acl || acl.computerId !== room.id || typeof msg.d !== 'string') {
      send(host.ws, JSON.stringify({ t: 'error', code: 'badAcl', ver: current }))
      return
    }
    if (acl.ver <= current) {
      send(host.ws, JSON.stringify({ t: 'error', code: 'staleVersion', ver: current }))
      return
    }
    this.store.saveAcl(room.id, acl.ver, msg.d)
    const now = this.now()
    for (const device of [...room.devices]) {
      if (device.pair) {
        if (!acl.offers.some((o) => o.id === device.offer && o.exp > now)) this.dropDevice(room, device, 4410, 'pairingExpired')
      } else if (!acl.devices.some((x) => x.dk === device.dk)) {
        this.dropDevice(room, device, 4003, 'revoked')
      }
    }
    send(host.ws, JSON.stringify({ t: 'acl.ok', ver: acl.ver }))
  }

  private onReady(room: Room, host: HostConn): void {
    if (host.left || room.host !== host || host.ready) return
    if (!this.readAcl(room.id)) {
      send(host.ws, JSON.stringify({ t: 'error', code: 'badAcl', ver: 0 }))
      return
    }
    host.ready = true
    room.lastSeenAt = this.now()
    const presence = JSON.stringify(this.presence(room))
    for (const device of room.devices) {
      send(device.ws, presence)
      send(host.ws, JSON.stringify({ t: 'open', link: device.link, dk: device.dk, pair: device.pair }))
    }
    this.deliver(room, host)
  }

  private onData(room: Room, host: HostConn, msg: Msg): void {
    if (!host.ready || room.host !== host || !msg.m || typeof msg.m !== 'object') return
    const device = room.devices.find((d) => d.link === msg.link)
    send(device?.ws, JSON.stringify(msg.m))
  }

  private onCloseDevice(room: Room, host: HostConn, msg: Msg): void {
    if (!host.ready || room.host !== host) return
    const device = room.devices.find((d) => d.link === msg.link)
    if (!device) return
    const code = validClose(msg.code) ? msg.code : 1000
    const reason = typeof msg.reason === 'string' ? msg.reason.slice(0, 120) : ''
    this.dropDevice(room, device, code, reason)
  }

  private onAck(room: Room, host: HostConn, msg: Msg): void {
    if (room.host !== host) return
    const note = ackMailbox(this.store, room.id, msg)
    if (note) this.note(room, [note])
    this.deliver(room, host)
  }

  private onDevice(room: Room, device: DeviceConn, raw: string): void {
    if (device.left) return
    const now = this.now()
    if (device.pair && now >= device.until) {
      this.dropDevice(room, device, 4410, 'pairingExpired')
      return
    }
    if (!this.charge(device, utf8(raw).length, now)) {
      this.dropDevice(room, device, 4008, 'rateLimited')
      return
    }
    if (device.pair) {
      device.msgs += 1
      if (device.msgs > PAIRING_MAX_MESSAGES) {
        this.dropDevice(room, device, 4008, 'rateLimited')
        return
      }
    }
    const msg = readMessage(raw)
    if (!msg) {
      this.dropDevice(room, device, 4002, 'protocolError')
      return
    }
    if (msg.t === 'ping') return send(device.ws, PONG)
    if (msg.t === 'hello' || msg.t === 'f') {
      const host = room.host
      if (host?.ready) send(host.ws, JSON.stringify({ t: 'data', link: device.link, m: msg }))
      else send(device.ws, JSON.stringify(this.presence(room)))
      return
    }
    if (msg.t === 'mbox.put' || msg.t === 'mbox.cancel' || msg.t === 'mbox.list') {
      if (device.pair) return send(device.ws, JSON.stringify({ t: 'mbox.err', nonce: msg.nonce ?? null, code: 'pairing' }))
      if (msg.t === 'mbox.put') return send(device.ws, putMailbox(this.store, room.id, device.dk, msg, room.host?.ready === true, now))
      if (msg.t === 'mbox.cancel') return send(device.ws, cancelMailbox(this.store, room.id, device.dk, msg))
      return send(device.ws, listMailbox(this.store, room.id, device.dk))
    }
    this.dropDevice(room, device, 4002, 'protocolError')
  }

  private charge(device: DeviceConn, bytes: number, now: number): boolean {
    if (now - device.rate.start >= RATE_WINDOW_MS) device.rate = { start: now, msgs: 0, bytes: 0 }
    const rate = device.rate
    rate.msgs += 1
    rate.bytes += bytes
    return rate.msgs <= RATE_MAX_MESSAGES && rate.bytes <= RATE_MAX_BYTES
  }

  private deliver(room: Room, host: HostConn): void {
    if (!host.ready || host.left || room.host !== host) return
    const text = deliverNext(this.store, room.id, this.now())
    if (text) send(host.ws, text)
  }

  private note(room: Room, notes: MailboxNote[]): void {
    for (const note of notes) {
      for (const device of room.devices) {
        if (device.dk === note.dk && !device.pair) send(device.ws, note.text)
      }
    }
  }

  private dropHost(room: Room, host: HostConn, code: number, reason: string, keepRoom = false): void {
    if (host.left) return
    host.left = true
    const wasReady = host.ready
    host.ready = false
    if (room.host === host) {
      room.host = null
      if (wasReady) {
        room.lastSeenAt = this.now()
        this.store.requeueDelivering(room.id)
        const presence = JSON.stringify(this.presence(room))
        for (const device of room.devices) send(device.ws, presence)
      }
    }
    this.close(host.ws, code, reason)
    // Replacing the host assigns the next socket on this same room. Dropping it first would leave
    // that host, and every later device, on different rooms.
    if (!keepRoom) this.release(room)
  }

  private dropDevice(room: Room, device: DeviceConn, code = 1000, reason = ''): void {
    if (device.left) return
    device.left = true
    room.devices = room.devices.filter((d) => d !== device)
    const host = room.host
    if (host?.ready) send(host.ws, JSON.stringify({ t: 'close', link: device.link }))
    this.close(device.ws, code, reason)
    this.release(room)
  }

  private release(room: Room): void {
    if (!room.host && room.devices.length === 0) this.rooms.delete(room.id)
  }

  private close(ws: WebSocket, code: number, reason: string): void {
    try {
      ws.close(code, reason)
    } catch {
      // Already gone.
    }
  }
}
