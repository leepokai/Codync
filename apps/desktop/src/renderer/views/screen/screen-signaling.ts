// How a viewer session gets its video: what the host's `screenPrepare` / `screenOffer` calls
// carry, and the order they run in. Complete-SDP negotiation, the phones' fallback: the
// loopback API this app uses has no `screenCandidates` stream (only the encrypted channel
// does), so the offer goes out once ICE gathering is done and carries every candidate.

import type { HostRoute } from '../../client/host-client.ts'

export interface IceServer { urls: string[]; username?: string; credential?: string }

/** A reserved viewer session and the ICE servers to reach it (`screenPrepare`). */
export interface ScreenConnection {
  session: string
  iceServers: IceServer[]
  /** Unix milliseconds. */
  expiresAt: number
  trickle: boolean
}

export interface ScreenAnswer { session: string; sdp: string }

/** The ICE servers are the cloud's TURN: wanted only when the host channel itself goes through the cloud. */
export function screenRelay(route: HostRoute | null): boolean {
  return route === 'relay'
}

/** Renew five minutes before the ICE credentials expire, never sooner than a second from now. */
export function renewIn(expiresAt: number, now: number): number {
  return Math.max(1_000, expiresAt - now - 300_000)
}

/** How long the offer waits for ICE gathering before sending what it has. */
export const GATHER_TIMEOUT_MS = 10_000
/** `screenOffer` waits for the helper to start capturing. */
export const OFFER_TIMEOUT_MS = 30_000

/** What a reconnect or an answer finds out of date: a newer connection or a close replaced this one. */
export class Superseded extends Error {
  constructor() { super('The screen connection was replaced.') }
}

/** The calls one negotiation makes; the session binds them to its peer and the host. */
export interface Negotiation {
  /** `screenPrepare`, with `trickle: false`. */
  prepare(relay: boolean): Promise<ScreenConnection>
  /** Creates the peer for this connection. */
  start(connection: ScreenConnection): void
  /** Offers, installs the local description and returns once gathering is complete (or timed out): the SDP to send. */
  offerSdp(): Promise<string>
  /** `screenOffer`. */
  offer(session: string, sdp: string): Promise<ScreenAnswer>
  /** Installs the answer. */
  accept(answer: ScreenAnswer): Promise<void>
  /** False once a newer connection or a close took over. */
  current(): boolean
}

/** prepare → peer → offer after gathering → answer. Throws `Superseded` as soon as it isn't current. */
export async function negotiate(n: Negotiation, relay: boolean): Promise<ScreenConnection> {
  const guard = () => { if (!n.current()) throw new Superseded() }
  const connection = await n.prepare(relay)
  guard()
  n.start(connection)
  const sdp = await n.offerSdp()
  guard()
  const answer = await n.offer(connection.session, sdp)
  guard()
  await n.accept(answer)
  guard()
  return { ...connection, session: answer.session || connection.session }
}

/** The host's replies on the `input` channel; anything else (an unknown type, bad JSON) is ignored. */
export type ScreenReply =
  | { type: 'clipboard'; text: string | null }
  | { type: 'view'; crop: number[] | null }
  | { type: 'error'; message: string | null }

export function decodeReply(text: string): ScreenReply | null {
  let message: unknown
  try { message = JSON.parse(text) } catch { return null }
  if (!message || typeof message !== 'object') return null
  const m = message as Record<string, unknown>
  const string = (v: unknown) => (typeof v === 'string' ? v : null)
  switch (m.type) {
    case 'clipboard': return { type: 'clipboard', text: string(m.text) }
    case 'view': return { type: 'view', crop: Array.isArray(m.crop) && m.crop.length === 4 && m.crop.every((n) => typeof n === 'number') ? (m.crop as number[]) : null }
    case 'error': return { type: 'error', message: string(m.message) }
    default: return null
  }
}

/** What the viewer asks the computer to stream: the whole display (or `display`) at this pixel size. */
export function viewRequest(display: number | null, pixels: { width: number; height: number }) {
  return {
    type: 'view' as const,
    long: Math.max(pixels.width, pixels.height),
    short: Math.min(pixels.width, pixels.height),
    ...(display !== null ? { display } : {}),
  }
}

/** Hardware H.264 first (High, then Constrained Baseline), every other codec only as a fallback. */
export function preferredCodecs<T extends { mimeType: string; sdpFmtpLine?: string }>(codecs: T[]): T[] {
  const h264 = codecs.filter((c) => c.mimeType.toLowerCase() === 'video/h264')
  const high = (c: T) => (c.sdpFmtpLine ?? '').includes('profile-level-id=640c')
  return [...h264.filter(high), ...h264.filter((c) => !high(c)), ...codecs.filter((c) => c.mimeType.toLowerCase() !== 'video/h264')]
}
