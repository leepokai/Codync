import type { CallError, HostSnapshot } from '@shared/ipc'
import { HostError, type HostTransport } from '../client/host-client'
import { COMPUTER_ID } from './story'
import { FakeHost } from './fake-host'

/**
 * The in-app demo's way to "reach" a host: calls go straight to the in-memory `FakeHost`. Nothing here touches the network,
 * the disk or the real host (`demo-isolation.test.ts` keeps it so).
 */
export class DemoTransport implements HostTransport {
  constructor(private readonly host: FakeHost) {}

  async call(method: string, body: unknown): Promise<unknown> {
    try {
      return await this.host.call(method, (body ?? {}) as Record<string, unknown>)
    } catch (e) {
      const error = e as CallError
      throw new HostError(error.status ?? 500, error.message ?? String(e))
    }
  }

  stream(path: string, onData: (line: string) => void) {
    const since = Number(new URL(path, 'http://demo.invalid').searchParams.get('since') ?? 0)
    return this.host.subscribe(since, onData)
  }
}

/** What the app believes about its host while in demo mode: a running one, with the demo's computer. */
export const DEMO_SNAPSHOT: HostSnapshot = {
  state: { kind: 'running' },
  local: { computerId: COMPUTER_ID, baseURL: 'http://demo.invalid', token: 'demo' },
  dev: false,
  logPath: '',
}
