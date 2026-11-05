/** A program a bot keeps running: the host runs it (`managed`), or the bot left it running through its shell. */
export interface DevServerPort {
  port: number
  /** Works on the computer running it. */
  local: string
  /** Work from other devices; empty when it listens only on that computer. */
  remote: string[]
}

export interface DevServer {
  id: string
  name: string
  command: string
  folder: string | null
  status: 'starting' | 'running' | 'stopped' | 'failed'
  ports: DevServerPort[]
  startedAt: number | null
  exitCode: number | null
  managed: boolean
}

const STATUSES: readonly string[] = ['starting', 'running', 'stopped', 'failed'] satisfies DevServer['status'][]

/** A status a newer host adds reads as stopped, rather than breaking the row. */
export function normalizeDevServer(raw: DevServer): DevServer {
  return STATUSES.includes(raw.status) ? raw : { ...raw, status: 'stopped' }
}

/** What the title shows of a bot's servers: nothing until one starts, or stays stopped. */
export type DevServerSummary =
  | { kind: 'none' }
  | { kind: 'starting' }
  | { kind: 'failed' }
  | { kind: 'running'; text: string }

export function devServerSummary(servers: DevServer[]): DevServerSummary {
  const running = servers.filter((s) => s.status === 'running')
  const first = running.find((s) => s.ports.length > 0) ?? running[0]
  if (first) {
    const label = first.ports.length > 0 ? `:${first.ports[0].port}` : first.name
    return { kind: 'running', text: running.length > 1 ? `${label} +${running.length - 1}` : label }
  }
  if (servers.some((s) => s.status === 'starting')) return { kind: 'starting' }
  if (servers.some((s) => s.status === 'failed')) return { kind: 'failed' }
  return { kind: 'none' }
}

/**
 * The address to open a running server at from here, if there is one. `onThisComputer`: the store is this app's own
 * computer. Anywhere else (a phone, or a desktop attached over SSH, whose loopback is a tunnel) can't use `local`.
 */
export function devServerAddress(server: DevServer, onThisComputer: boolean): string | null {
  const port = server.ports[0]
  if (server.status !== 'running' || !port) return null
  return onThisComputer ? port.local : port.remote[0] ?? null
}

/** Its row's value: its ports, or a phrase to look up. */
export function devServerValue(server: DevServer): string {
  switch (server.status) {
    case 'starting': return 'Starting'
    case 'failed': return 'Failed to start'
    case 'stopped': return 'Stopped'
    case 'running': return server.ports.length > 0 ? server.ports.map((p) => `:${p.port}`).join(' ') : 'Running'
  }
}

export type DevServerClick = { kind: 'open'; address: string } | { kind: 'logs' } | { kind: 'menu' }

/** What clicking its row does: open it, or read its log when it has no port (a watch build, or one that ended). */
export function devServerClick(server: DevServer, onThisComputer: boolean): DevServerClick {
  const address = devServerAddress(server, onThisComputer)
  if (address) return { kind: 'open', address }
  const listens = server.status === 'running' && server.ports.length > 0
  return server.managed && !listens ? { kind: 'logs' } : { kind: 'menu' }
}

export type DevServerMenuRow = 'copyAddress' | 'logs' | 'restart' | 'start' | 'stop' | 'remove'

/** Only what applies. A server the bot started through its shell can only be opened or stopped. */
export function devServerMenuRows(server: DevServer): DevServerMenuRow[] {
  const live = server.status === 'running' || server.status === 'starting'
  const rows: DevServerMenuRow[] = []
  if (live && server.ports.length > 0) rows.push('copyAddress')
  if (server.managed) {
    rows.push('logs')
    if (server.status !== 'starting') rows.push(live ? 'restart' : 'start')
  }
  if (live) rows.push('stop')
  else if (server.managed) rows.push('remove')
  return rows
}
