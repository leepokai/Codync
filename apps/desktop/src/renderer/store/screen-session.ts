import type { ScreenDisplay } from '@shared/models'
import type { HostClient } from '../client/host-client'
import { errorText } from '../lib/error-text.ts'
import { Observable } from '../lib/observable.ts'
import type { ScreenInput } from '../lib/screen-input'

// One live view of a computer's screen (the iPhone's `ScreenSession`): a WebRTC peer that receives the display as
// H.264 and sends input over two data channels (`input` reliable, `input-fast` unordered for pointer moves).
// Signaling goes through the host API with the complete offer, so it needs no candidate stream on either route.

export type ScreenPhase =
  | { kind: 'connecting' }
  | { kind: 'live' }
  | { kind: 'reconnecting' }
  | { kind: 'failed'; message: string }
  | { kind: 'closed' }

interface Prepared {
  session: string
  iceServers: RTCIceServer[]
  expiresAt: number
}

const MAX_RECOVERY = 3
/** How long the offer waits for its candidates (TURN over TCP/TLS can be slow). */
const GATHER_MS = 10_000
const CONNECT_MS = 30_000
/** Reconnect with fresh TURN credentials this long before they expire. */
const RENEW_BEFORE_MS = 300_000
const CONNECT_FAILED = "Couldn't connect to the remote screen. Check the computer's connection and try again."

/** A newer peer or a close retired the one this step worked on. */
class Retired extends Error {}

export class ScreenSession extends Observable {
  phase: ScreenPhase = { kind: 'connecting' }
  video: MediaStream | null = null
  /** Text the computer copied, waiting for the user to take it. */
  remoteClipboard: string | null = null
  lastError: string | null = null
  #session: string | null = null
  #pc: RTCPeerConnection | null = null
  #reliable: RTCDataChannel | null = null
  #fast: RTCDataChannel | null = null
  /** Reconnects before the TURN credentials expire. */
  #renewal: ReturnType<typeof setTimeout> | null = null
  /** Gives up on a peer that never connects; cleared once it does. */
  #connectTimer: ReturnType<typeof setTimeout> | null = null
  #recovery: ReturnType<typeof setTimeout> | null = null
  #attempts = 0
  /** The size last asked for: sent again once a new peer's channel opens, which drops what came before. */
  #shown: { long: number; short: number } | null = null

  readonly #client: HostClient
  /** Through Codync's relay: the host hands out TURN servers, since a direct path may not exist. */
  readonly #relay: boolean
  display: ScreenDisplay

  constructor(client: HostClient, relay: boolean, display: ScreenDisplay) {
    super()
    this.#client = client
    this.#relay = relay
    this.display = display
  }

  async start() {
    this.#attempts = 0
    this.#set({ kind: 'connecting' })
    try {
      await this.#connect()
    } catch (error) {
      if (this.phase.kind === 'connecting' && !(error instanceof Retired)) this.#fail(errorText(error))
    }
  }

  close() {
    this.#cancelRecovery()
    this.#set({ kind: 'closed' })
    if (this.#session) this.#closeSession(this.#session)
    this.#session = null
    this.#teardown()
  }

  /** One input event (display points). A late pointer move is useless, so moves go unordered. */
  send(event: ScreenInput) {
    const fast = event.type === 'move' && this.#fast?.readyState === 'open' ? this.#fast : null
    const channel = fast ?? this.#reliable
    if (channel?.readyState === 'open') channel.send(JSON.stringify(event))
  }

  /** The size the picture is shown at, in device pixels, so the computer sends no more than that. */
  view(long: number, short: number) {
    this.#shown = { long: Math.round(long), short: Math.round(short) }
    this.send({ type: 'view', display: this.display.id, ...this.#shown })
  }

  switchDisplay(display: ScreenDisplay) {
    this.display = display
    this.send({ type: 'view', display: display.id })
    this.changed()
  }

  pushClipboard(text: string) {
    this.send({ type: 'clipboard', text })
  }

  takeRemoteClipboard(): string | null {
    const text = this.remoteClipboard
    this.remoteClipboard = null
    this.changed()
    return text
  }

  async #connect() {
    const pending = this.#pc
    const prepared = await this.#client.call<Prepared>('screenPrepare', { relay: this.#relay, trickle: false })
    if (this.#pc !== pending || this.phase.kind === 'closed') {
      this.#closeSession(prepared.session)
      throw new Retired()
    }
    // Reserve the replacement before closing the old viewer, so a takeover survives the renewal.
    const old = this.#session
    this.#teardown()
    this.#session = prepared.session
    if (old) this.#closeSession(old)
    const pc = new RTCPeerConnection({ iceServers: prepared.iceServers, bundlePolicy: 'max-bundle' })
    this.#pc = pc
    // Set before negotiating: on a fast path ICE connects before the answer's promise settles.
    this.#connectTimer = setTimeout(() => {
      if (this.#pc === pc && this.phase.kind !== 'live') this.#fail(CONNECT_FAILED)
    }, CONNECT_MS)
    preferH264(pc.addTransceiver('video', { direction: 'recvonly' }))
    pc.ontrack = (e) => {
      if (this.#pc !== pc) return
      this.video = e.streams[0] ?? new MediaStream([e.track])
      this.changed()
    }
    this.#reliable = pc.createDataChannel('input')
    this.#fast = pc.createDataChannel('input-fast', { ordered: false, maxRetransmits: 0 })
    this.#reliable.onmessage = (e) => {
      if (this.#pc === pc && typeof e.data === 'string') this.#received(e.data)
    }
    this.#reliable.onopen = () => {
      if (this.#pc === pc && this.#shown) this.view(this.#shown.long, this.#shown.short)
    }
    pc.oniceconnectionstatechange = () => {
      if (this.#pc === pc) this.#iceChanged(pc.iceConnectionState)
    }

    await pc.setLocalDescription(await pc.createOffer())
    await gathered(pc, GATHER_MS)
    this.#ensure(pc)
    const answer = await this.#client.call<{ sdp: string }>('screenOffer', { sdp: pc.localDescription?.sdp, session: prepared.session, display: this.display.id }, 30_000)
    this.#ensure(pc)
    await pc.setRemoteDescription({ type: 'answer', sdp: answer.sdp })
    this.#ensure(pc)
    this.#renewal = setTimeout(() => {
      if (this.#pc !== pc) return
      this.#set({ kind: 'reconnecting' })
      this.#recover(0)
    }, Math.max(1000, prepared.expiresAt - Date.now() - RENEW_BEFORE_MS))
  }

  #ensure(pc: RTCPeerConnection) {
    if (this.#pc !== pc || this.phase.kind === 'closed') throw new Retired()
  }

  #iceChanged(state: RTCIceConnectionState) {
    if (this.phase.kind === 'closed') return
    if (state === 'connected' || state === 'completed') {
      this.#clearConnectTimer()
      this.#attempts = 0
      this.lastError = null
      this.#set({ kind: 'live' })
    } else if (state === 'disconnected') {
      // Often recovers by itself (a network change); reconnect if it doesn't.
      this.#set({ kind: 'reconnecting' })
      this.#recover(2000)
    } else if (state === 'failed') {
      this.#set({ kind: 'reconnecting' })
      this.#recover(0)
    }
  }

  #recover(delay: number) {
    this.#cancelRecovery()
    this.#recovery = setTimeout(() => {
      this.#recovery = null
      void this.#recoverNow()
    }, delay)
  }

  async #recoverNow() {
    while (this.phase.kind === 'reconnecting') {
      if (this.#attempts >= MAX_RECOVERY) return this.#fail(this.lastError ?? CONNECT_FAILED)
      this.#attempts++
      try {
        await this.#connect()
        return
      } catch (error) {
        // A close while it waited; the narrowed phase above can't see it.
        if ((this.phase as ScreenPhase).kind === 'closed') return
        if (!(error instanceof Retired)) this.lastError = errorText(error)
        await new Promise((resolve) => setTimeout(resolve, Math.min(this.#attempts * 2000, 10_000)))
      }
    }
  }

  #cancelRecovery() {
    if (this.#recovery) clearTimeout(this.#recovery)
    this.#recovery = null
  }

  #received(data: string) {
    let msg: { type?: string; text?: unknown; message?: unknown }
    try {
      msg = JSON.parse(data) as typeof msg
    } catch {
      return
    }
    if (msg.type === 'clipboard' && typeof msg.text === 'string') this.remoteClipboard = msg.text
    else if (msg.type === 'error' && typeof msg.message === 'string') this.lastError = msg.message
    else return
    this.changed()
  }

  #fail(message: string) {
    this.close()
    this.#set({ kind: 'failed', message })
  }

  #set(phase: ScreenPhase) {
    this.phase = phase
    this.changed()
  }

  #closeSession(session: string) {
    this.#client.call('screenClose', { session }).catch(() => {})
  }

  #clearConnectTimer() {
    if (this.#connectTimer) clearTimeout(this.#connectTimer)
    this.#connectTimer = null
  }

  #teardown() {
    if (this.#renewal) clearTimeout(this.#renewal)
    this.#renewal = null
    this.#clearConnectTimer()
    this.#reliable?.close()
    this.#fast?.close()
    this.#pc?.close()
    this.#reliable = null
    this.#fast = null
    this.#pc = null
    this.video = null
    this.changed()
  }
}

/** Hardware H.264 first (High, then the rest), other codecs only as a fallback. */
function preferH264(transceiver: RTCRtpTransceiver) {
  const codecs = RTCRtpReceiver.getCapabilities('video')?.codecs ?? []
  const isH264 = (c: RTCRtpCodec) => c.mimeType.toLowerCase() === 'video/h264'
  const isHigh = (c: RTCRtpCodec) => c.sdpFmtpLine?.includes('profile-level-id=640c') === true
  const h264 = codecs.filter(isH264).sort((a, b) => Number(isHigh(b)) - Number(isHigh(a)))
  if (h264.length) transceiver.setCodecPreferences([...h264, ...codecs.filter((c) => !isH264(c))])
}

/** Resolves once the offer has its candidates, or after `ms` with what it has. */
function gathered(pc: RTCPeerConnection, ms: number) {
  return new Promise<void>((resolve) => {
    if (pc.iceGatheringState === 'complete') return resolve()
    const done = () => {
      clearTimeout(timer)
      pc.removeEventListener('icegatheringstatechange', check)
      resolve()
    }
    const check = () => {
      if (pc.iceGatheringState === 'complete') done()
    }
    const timer = setTimeout(done, ms)
    pc.addEventListener('icegatheringstatechange', check)
  })
}
