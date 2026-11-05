import { Observable } from '../../lib/observable'
import type { ScreenDisplay } from '@shared/models'
import type { InputMessage } from './screen-input'
import type { ScreenLink } from './screen-link'
import {
  CONNECT_TIMEOUT_MS, connectTimedOut, DISCONNECTED_GRACE_MS, messageOf, recover, ROUTE_TIMEOUT_MS, statusOf, terminalStatuses, Unreachable,
  type Failure, type Recovery,
} from './screen-recovery'
import {
  decodeReply, GATHER_TIMEOUT_MS, negotiate, OFFER_TIMEOUT_MS, preferredCodecs, renewIn, screenRelay, Superseded, viewRequest,
  type Negotiation, type ScreenConnection,
} from './screen-signaling'

export type Phase =
  | { kind: 'connecting' }
  | { kind: 'live' }
  | { kind: 'reconnecting' }
  | { kind: 'failed'; failure: Failure }
  | { kind: 'closed' }

/**
 * One live view of a computer's screen (the phones' `ScreenSession`): a WebRTC peer that
 * receives the display and sends input over two data channels (`input` reliable, `input-fast`
 * unordered for pointer moves). Reconnects on a fresh session, up to three times.
 */
export class ScreenSession extends Observable {
  phase: Phase = { kind: 'connecting' }
  stream: MediaStream | null = null
  /** Text the computer copied, waiting for the user to pull it onto this computer. */
  remoteClipboard: string | null = null
  lastError: string | null = null
  /** The display the video shows; null for the computer's main one. */
  display: ScreenDisplay | null

  /** The connection is about to be replaced or closed: lift whatever input holds. */
  onRelease: () => void = () => {}
  /** The live session changed (null: none); the window tells the main process. */
  onSession: (session: string | null) => void = () => {}

  private link: ScreenLink
  private session: string | null = null
  private pc: RTCPeerConnection | null = null
  private reliable: RTCDataChannel | null = null
  private fast: RTCDataChannel | null = null
  /** Bumped by every connect, close and failure: a negotiation still running for an older number is dropped. */
  private attempt = 0
  private recovering = 0
  private recovery = { attempts: 0 }
  private timers = new Set<ReturnType<typeof setTimeout>>()
  private renew: ReturnType<typeof setTimeout> | null = null
  private connectTimer: ReturnType<typeof setTimeout> | null = null
  private pixels: { width: number; height: number } | null = null

  constructor(link: ScreenLink, display: ScreenDisplay | null) {
    super()
    this.link = link
    this.display = display
  }

  // MARK: connection

  /** Starts (or restarts after an error) with a full set of reconnect attempts. */
  start() {
    this.recovering++
    this.recovery.attempts = 0
    this.phase = { kind: 'connecting' }
    this.lastError = null
    this.changed()
    this.connect().catch((error) => {
      if (error instanceof Superseded || this.phase.kind !== 'connecting') return
      this.fail(messageOf(error) ? { reason: 'host', detail: messageOf(error) } : { reason: 'connect', detail: null })
    })
  }

  private async connect() {
    const mine = ++this.attempt
    const current = () => mine === this.attempt && this.phase.kind !== 'closed'
    let peer: RTCPeerConnection | null = null
    const n: Negotiation = {
      prepare: async (relay) => {
        const connection = await this.link.call<ScreenConnection>('screenPrepare', { relay, trickle: false })
        // Replaced while preparing: give the reservation back.
        if (!current()) this.closeRemote(connection.session)
        return connection
      },
      start: (connection) => {
        // The replacement is reserved before the old viewer closes, so a takeover survives renewal.
        const old = this.session
        this.onRelease()
        this.teardown()
        this.setSession(connection.session)
        if (old) this.closeRemote(old)
        peer = this.createPeer(connection)
      },
      offerSdp: async () => {
        const pc = peer
        if (!pc) throw new Superseded()
        const offer = await pc.createOffer()
        if (!current() || this.pc !== pc) throw new Superseded()
        await pc.setLocalDescription(offer)
        await gathered(pc, GATHER_TIMEOUT_MS)
        if (!current() || this.pc !== pc) throw new Superseded()
        return pc.localDescription?.sdp ?? offer.sdp ?? ''
      },
      offer: (session, sdp) => this.link.call('screenOffer', { sdp, session, ...(this.display ? { display: this.display.id } : {}) }, OFFER_TIMEOUT_MS),
      accept: async (answer) => {
        const pc = peer
        if (!pc || this.pc !== pc) throw new Superseded()
        await pc.setRemoteDescription({ type: 'answer', sdp: answer.sdp })
      },
      current,
    }
    const connection = await negotiate(n, screenRelay(this.link.route))
    this.setSession(connection.session)
    // Refresh both endpoints before the ICE credentials expire, including an idle screen.
    this.renew = this.later(renewIn(connection.expiresAt, Date.now()), () => {
      if (!current()) return
      this.phase = { kind: 'reconnecting' }
      this.changed()
      this.startRecovery(0)
    })
    this.connectTimer = this.later(CONNECT_TIMEOUT_MS, () => {
      if (!current() || this.phase.kind === 'live') return
      connectTimedOut(this.phase.kind === 'reconnecting', { retry: () => this.startRecovery(0), fail: (failure) => this.fail(failure) })
    })
  }

  private createPeer(connection: ScreenConnection): RTCPeerConnection {
    const pc = new RTCPeerConnection({
      iceServers: connection.iceServers,
      bundlePolicy: 'max-bundle',
    })
    this.pc = pc
    const video = pc.addTransceiver('video', { direction: 'recvonly' })
    try {
      video.setCodecPreferences(preferredCodecs(RTCRtpReceiver.getCapabilities('video')?.codecs ?? []))
    } catch (error) {
      // The default order still negotiates; only the hardware H.264 preference is lost.
      console.warn(`screen: codec preference not applied: ${messageOf(error) ?? error}`)
    }
    pc.ontrack = (event) => {
      if (this.pc !== pc) return
      this.stream = event.streams[0] ?? new MediaStream([event.track])
      this.changed()
    }
    const reliable = pc.createDataChannel('input')
    reliable.onopen = () => { if (this.reliable === reliable) this.sendView() }
    reliable.onmessage = (event) => { if (this.reliable === reliable && typeof event.data === 'string') this.received(event.data) }
    this.reliable = reliable
    this.fast = pc.createDataChannel('input-fast', { ordered: false, maxRetransmits: 0 })
    pc.oniceconnectionstatechange = () => { if (this.pc === pc) this.iceChanged(pc.iceConnectionState) }
    return pc
  }

  private iceChanged(state: RTCIceConnectionState) {
    if (this.phase.kind === 'closed' || this.phase.kind === 'failed') return
    switch (state) {
      case 'connected':
      case 'completed':
        this.recovering++
        this.recovery.attempts = 0
        if (this.connectTimer) this.clear(this.connectTimer)
        this.connectTimer = null
        this.phase = { kind: 'live' }
        this.lastError = null
        this.changed()
        break
      // Often recovers by itself (Wi-Fi to another network); reconnect if it does not.
      case 'disconnected':
        this.phase = { kind: 'reconnecting' }
        this.changed()
        this.startRecovery(DISCONNECTED_GRACE_MS)
        break
      case 'failed':
        this.phase = { kind: 'reconnecting' }
        this.changed()
        this.startRecovery(0)
        break
      default:
        break
    }
  }

  // MARK: recovery

  private startRecovery(firstDelay: number) {
    const mine = ++this.recovering
    const active = () => mine === this.recovering && this.phase.kind === 'reconnecting'
    const r: Recovery = {
      active,
      route: async () => {
        // Wait for a usable route before spending an attempt: the host may be restarting, or a tunnel reconnecting.
        const since = Date.now()
        while (active()) {
          try {
            await this.link.call('screenStatus', {}, 5_000)
            return
          } catch (error) {
            const status = statusOf(error)
            if (status !== null && terminalStatuses.includes(status)) throw error
            this.lastError = messageOf(error)
            if (Date.now() - since >= ROUTE_TIMEOUT_MS) throw new Unreachable()
            await this.sleep(1_000)
          }
        }
      },
      reconnect: () => this.connect(),
      sleep: (ms) => this.sleep(ms),
      fail: (failure) => this.fail(failure),
    }
    void recover(r, this.recovery, firstDelay, () => this.lastError)
  }

  private sleep(ms: number) {
    return new Promise<void>((resolve) => this.later(ms, resolve))
  }

  private later(ms: number, work: () => void) {
    const timer = setTimeout(() => {
      this.timers.delete(timer)
      work()
    }, ms)
    this.timers.add(timer)
    return timer
  }

  private clear(timer: ReturnType<typeof setTimeout>) {
    clearTimeout(timer)
    this.timers.delete(timer)
  }

  // MARK: ending

  /** Ends the view: lifts held input, closes the session on the computer, frees the peer. */
  close() {
    if (this.phase.kind === 'closed') return
    this.end({ kind: 'closed' })
  }

  private fail(failure: Failure) {
    if (this.phase.kind === 'closed' || this.phase.kind === 'failed') return
    this.end({ kind: 'failed', failure })
  }

  private end(phase: Phase) {
    this.onRelease()
    this.attempt++
    this.recovering++
    this.phase = phase
    const session = this.session
    this.session = null
    // The window learns there is nothing to close only once the computer has been told.
    if (session) this.closeRemote(session, () => { if (this.session === null) this.onSession(null) })
    else this.onSession(null)
    this.teardown()
    this.stream = null
    this.changed()
  }

  private setSession(session: string) {
    if (this.session === session) return
    this.session = session
    this.onSession(session)
  }

  /** Best effort: a viewer whose close never arrives ends with its WebRTC connection. */
  private closeRemote(session: string, settled: () => void = () => {}) {
    this.link.callNow('screenClose', { session }, 15_000)
      .catch((error) => console.info(`screen close failed: ${messageOf(error) ?? error}`))
      .finally(settled)
  }

  private teardown() {
    if (this.renew) this.clear(this.renew)
    if (this.connectTimer) this.clear(this.connectTimer)
    this.renew = null
    this.connectTimer = null
    const { pc, reliable, fast } = this
    this.pc = null
    this.reliable = null
    this.fast = null
    for (const channel of [reliable, fast]) {
      if (!channel) continue
      channel.onopen = null
      channel.onmessage = null
      channel.close()
    }
    if (pc) {
      pc.ontrack = null
      pc.oniceconnectionstatechange = null
      pc.close()
    }
  }

  // MARK: input

  /** Sends one input message (display points). Pointer moves go unordered: a late move is useless. */
  send(message: InputMessage | ReturnType<typeof viewRequest> | { type: 'clipboard'; text: string }) {
    const fast = message.type === 'move' && this.fast?.readyState === 'open'
    const channel = fast ? this.fast : this.reliable
    if (channel?.readyState !== 'open') return
    channel.send(JSON.stringify(message))
  }

  /** What to stream: the display at the viewer's size in device pixels. Sent now and whenever the channel (re)opens. */
  setPixels(pixels: { width: number; height: number }) {
    this.pixels = pixels
    this.sendView()
  }

  switchDisplay(display: ScreenDisplay) {
    this.display = display
    this.sendView()
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

  private sendView() {
    if (this.pixels) this.send(viewRequest(this.display?.id ?? null, this.pixels))
  }

  private received(text: string) {
    const reply = decodeReply(text)
    if (!reply) return
    if (reply.type === 'clipboard') this.remoteClipboard = reply.text
    else if (reply.type === 'error') this.lastError = reply.message
    // `view` (the streamed region) only matters when zooming, which this viewer doesn't do.
    this.changed()
  }
}

/** Resolves once ICE gathering is complete, or after `timeoutMs` (the offer then carries what was found). */
function gathered(pc: RTCPeerConnection, timeoutMs: number): Promise<void> {
  if (pc.iceGatheringState === 'complete') return Promise.resolve()
  return new Promise((resolve) => {
    const done = () => {
      clearTimeout(timer)
      pc.removeEventListener('icegatheringstatechange', check)
      resolve()
    }
    const check = () => { if (pc.iceGatheringState === 'complete') done() }
    const timer = setTimeout(done, timeoutMs)
    pc.addEventListener('icegatheringstatechange', check)
  })
}
