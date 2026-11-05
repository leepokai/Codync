// Losing the connection: up to three reconnects, each on a fresh session, then an error with
// Retry (the Android `ScreenSession.scheduleRecovery`).

/** Statuses that end recovery at once: access was taken away, or the app must update. */
export const terminalStatuses = [403, 426]
export const maxRecoveryAttempts = 3
/** The first ICE `disconnected` often heals by itself (Wi-Fi to cellular): reconnect only if it doesn't. */
export const DISCONNECTED_GRACE_MS = 2_000
/** A connection that isn't live by then fails. */
export const CONNECT_TIMEOUT_MS = 30_000

/** The computer can't be reached at all (its tunnel never came back): no attempt was spent. */
export class Unreachable extends Error {
  constructor() { super("Couldn't reach the computer.") }
}

/** How long a lost route is waited for before giving up. */
export const ROUTE_TIMEOUT_MS = 30_000

/**
 * Why the viewer stopped. `host` carries the host's own text, shown as it wrote it; the other
 * reasons are this app's wording, translated where they're shown.
 */
export interface Failure {
  reason: 'connect' | 'reconnect' | 'unreachable' | 'host'
  detail: string | null
}

export const hostFailure = (detail: string | null): Failure => (detail ? { reason: 'host', detail } : { reason: 'reconnect', detail: null })

export function statusOf(error: unknown): number | null {
  const status = (error as { status?: unknown } | null)?.status
  return typeof status === 'number' ? status : null
}

export function messageOf(error: unknown): string | null {
  const message = (error as { message?: unknown } | null)?.message
  return typeof message === 'string' && message ? message : null
}

/** Waits grow with each failed attempt: 2 s, 4 s, 6 s, never past 10 s. */
export function retryDelay(attempts: number): number {
  return Math.min(Math.max(1, attempts) * 2_000, 10_000)
}

/**
 * A connection that isn't live in time. The first connection fails (Try again); a reconnect hands over to the
 * next attempt, which fails only when the attempts are spent (`recover` counts them).
 */
export function connectTimedOut(reconnecting: boolean, r: { retry(): void; fail(failure: Failure): void }) {
  if (reconnecting) r.retry()
  else r.fail({ reason: 'connect', detail: null })
}

export interface Recovery {
  /** The session is still reconnecting (not closed, not live again). */
  active(): boolean
  /** Waits until the computer's host answers again; throws a terminal error if access is gone. */
  route(): Promise<void>
  /** One new connection. */
  reconnect(): Promise<void>
  sleep(ms: number): Promise<void>
  /** Out of attempts, or a terminal error. */
  fail(failure: Failure): void
}

/**
 * Runs the attempts. `attempts` counts the ones already spent (reset by the session only when
 * ICE connects: a successful answer alone doesn't prove the video flows). Returns when the
 * connection is back, the session stopped needing it, or `fail` was called.
 */
export async function recover(r: Recovery, state: { attempts: number }, firstDelay: number, lastError: () => string | null): Promise<void> {
  await r.sleep(firstDelay)
  while (r.active()) {
    if (state.attempts >= maxRecoveryAttempts) return r.fail(hostFailure(lastError()))
    try {
      await r.route()
    } catch (error) {
      if (r.active()) r.fail(error instanceof Unreachable ? { reason: 'unreachable', detail: null } : hostFailure(messageOf(error)))
      return
    }
    if (!r.active()) return
    state.attempts += 1
    try {
      await r.reconnect()
      return
    } catch (error) {
      if (!r.active()) return
      const status = statusOf(error)
      // The host answers most errors with 400 (a restarting helper, a replaced session): only a denial or a
      // required update ends recovery early.
      if ((status !== null && terminalStatuses.includes(status)) || state.attempts >= maxRecoveryAttempts) {
        return r.fail(hostFailure(messageOf(error) ?? lastError()))
      }
    }
    if (!r.active()) return
    await r.sleep(retryDelay(state.attempts))
  }
}
