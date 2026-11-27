// The outside apps a computer lets drive its work over MCP, as the host lists them (`mcpClients`): docs/features/mcp-access.md.

export type McpLevel = 'readOnly' | 'ask' | 'auto' | 'full'

export const mcpLevels: { id: McpLevel; label: string }[] = [
  { id: 'readOnly', label: 'Read only' },
  { id: 'ask', label: 'Ask for every request' },
  { id: 'auto', label: 'Auto' },
  { id: 'full', label: 'Full access' },
]

export interface McpClient {
  id: string
  label: string
  access: McpLevel
  scopes: string[]
  bots: string[]
  plainThreads: boolean
  createdAt: number
  expiresAt: number
  lastUsedAt: number | null
  revokedAt: number | null
}

export interface McpLevels {
  max: McpLevel
  local: McpLevel
  cloud: McpLevel
}

export interface McpAccess {
  clients: McpClient[]
  levels: McpLevels
}

/** The list as the host sends it; an app it can't read is left out, never the whole list. */
export function readMcpAccess(raw: unknown): McpAccess {
  const v = (raw ?? {}) as { clients?: unknown[]; levels?: Partial<McpLevels> }
  const level = (l: unknown, fallback: McpLevel): McpLevel => (mcpLevels.some((x) => x.id === l) ? (l as McpLevel) : fallback)
  const clients = (v.clients ?? []).flatMap((c): McpClient[] => {
    const x = c as Partial<McpClient>
    if (typeof x.id !== 'string' || typeof x.label !== 'string') return []
    return [{
      id: x.id, label: x.label, access: level(x.access, 'readOnly'), scopes: x.scopes ?? [], bots: x.bots ?? [],
      plainThreads: x.plainThreads === true, createdAt: x.createdAt ?? 0, expiresAt: x.expiresAt ?? 0,
      lastUsedAt: x.lastUsedAt ?? null, revokedAt: x.revokedAt ?? null,
    }]
  })
  return {
    clients,
    levels: { max: level(v.levels?.max, 'auto'), local: level(v.levels?.local, 'ask'), cloud: level(v.levels?.cloud, 'readOnly') },
  }
}

/** The apps still let in, newest first. */
export function activeClients(access: McpAccess): McpClient[] {
  return access.clients.filter((c) => c.revokedAt == null).sort((a, b) => b.createdAt - a.createdAt)
}

/** How many whole days are left, 0 once it expired. */
export function daysLeft(client: McpClient, now = Date.now()): number {
  return Math.max(0, Math.ceil((client.expiresAt - now) / 86_400_000))
}

/** The line under an app: until when it works. */
export function clientDetail(client: McpClient, tr: (text: string) => string, now = Date.now()): string {
  const left = daysLeft(client, now)
  if (left === 0) return tr('Expired')
  return tr('Expires in {count} days').replace('{count}', String(left))
}
