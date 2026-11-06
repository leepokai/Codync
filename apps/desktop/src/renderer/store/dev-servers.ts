import type { HostClient } from '../client/host-client'

type DevServerMethod = 'devServerStop' | 'devServerRestart' | 'devServerRemove'

/** What this app can ask of a computer's dev servers: it names one, and never sends a command. */
export class DevServerCommands {
  private perform: (op: (c: HostClient) => Promise<unknown>) => Promise<boolean>
  private withLink: <T>(op: (c: HostClient) => Promise<T>) => Promise<T>
  constructor(perform: DevServerCommands['perform'], withLink: DevServerCommands['withLink']) {
    this.perform = perform
    this.withLink = withLink
  }

  act(method: DevServerMethod, botId: string, id: string) {
    return this.perform((c) => c.call(method, { botId, id }))
  }

  async logs(botId: string, id: string): Promise<string> {
    const { text } = await this.withLink((c) => c.call<{ text: string }>('devServerLogs', { botId, id }))
    return text
  }
}
