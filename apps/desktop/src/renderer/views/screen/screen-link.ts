import { HostError, LoopbackTransport, type HostRoute } from '../../client/host-client'

/** The loopback host of one computer: this one, or one reached through an SSH tunnel. */
export interface ScreenTarget {
  baseURL: string
  token: string
}

/**
 * Calls a computer's host. The target is looked up on every call: a tunnel that reconnects
 * comes back on another port, and a viewer left open across that keeps working.
 */
export class ScreenLink {
  /** Every route this app has is the loopback API, never the cloud channel (so no TURN). */
  readonly route: HostRoute = 'loopback'
  readonly computerId: string
  private latest: ScreenTarget | null = null

  constructor(computerId: string) {
    this.computerId = computerId
  }

  /** The target at the last call, for the main process's safety close. */
  get target(): ScreenTarget | null {
    return this.latest
  }

  async resolve(): Promise<ScreenTarget> {
    const snapshot = await window.codync.host.snapshot()
    if (snapshot.local?.computerId === this.computerId) return this.remember(snapshot.local)
    const attachment = (await window.codync.ssh.state()).attachments.find((a) => a.computer.id === this.computerId)
    if (attachment) return this.remember(attachment)
    throw HostError.unreachable()
  }

  async call<T>(method: string, body: unknown = {}, timeoutMs = 20_000): Promise<T> {
    const { baseURL, token } = await this.resolve()
    return (await new LoopbackTransport(baseURL, token).call(method, body, timeoutMs)) as T
  }

  /**
   * Like `call` without looking the target up first: a window that is closing can't wait for two
   * round trips to the main process before its last request has to leave.
   */
  async callNow<T>(method: string, body: unknown = {}, timeoutMs = 20_000): Promise<T> {
    const target = this.latest ?? (await this.resolve())
    return (await new LoopbackTransport(target.baseURL, target.token).call(method, body, timeoutMs)) as T
  }

  private remember({ baseURL, token }: ScreenTarget): ScreenTarget {
    this.latest = { baseURL, token }
    return this.latest
  }
}
