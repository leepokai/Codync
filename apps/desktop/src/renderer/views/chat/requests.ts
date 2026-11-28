import type { Entry, PermissionOption } from '@shared/models'

const hasDuplicateKind = (kind: string, options?: PermissionOption[]) =>
  (options?.filter((o) => o.kind === kind).length ?? 0) > 1

const trimmedName = (name?: string | null) => (typeof name === 'string' ? name.trim() : '')

/**
 * Label for a permission option: Codync's own words when each kind appears once, or the agent's own option name when
 * multiple options share a kind (e.g. Codex MCP sending two `allow_always` scopes).
 */
export function permissionOptionLabel(o: PermissionOption, options: PermissionOption[] | undefined, tr: (s: string) => string): string {
  const name = trimmedName(o.name)
  if (hasDuplicateKind(o.kind, options) && name) return name
  switch (o.kind) {
    case 'allow_once':
      return tr('Allow once')
    case 'allow_always':
      return tr('Always allow')
    case 'reject_once':
      return tr('Deny')
    case 'reject_always':
      return tr('Never')
    default:
      return typeof o.name === 'string' ? o.name : ''
  }
}

/** Label for a settled permission card; keeps the agent's own name when multiple options share the chosen kind. */
export function permissionOutcomeLabel(
  status: string | undefined,
  selected: string | undefined,
  options: PermissionOption[] | undefined,
  tr: (s: string) => string,
): string {
  switch (status) {
    case 'answered': {
      const chosen = options?.find((o) => o.optionId === selected)
      const chosenName = trimmedName(chosen?.name)
      if (chosen && hasDuplicateKind(chosen.kind, options) && chosenName) {
        return chosenName
      }
      switch (chosen?.kind) {
        case 'allow_once':
          return tr('Allowed once')
        case 'allow_always':
          return tr('Always allowed')
        case 'reject_once':
          return tr('Denied')
        case 'reject_always':
          return tr('Never allowed')
        default:
          return (typeof chosen?.name === 'string' ? chosen.name : null) ?? tr('Answered')
      }
    }
    case 'cancelled':
      return tr('Cancelled')
    default:
      return tr('Expired')
  }
}

/**
 * How a request reads in a line: an MCP tool's call ("memory-read_memory: read_memory") as its tool's name in words
 * and the server it comes from; anything else (a command, a file) as the agent wrote it, literal.
 */
export function requestName(title: string): { name: string; source: string | null; literal: boolean } {
  const tool = /^(.+)-(\w+): \2$/.exec(title.trim())
  if (!tool) return { name: title.trim().replace(/^`([^`]*)`$/, '$1'), source: null, literal: true }
  const words = tool[2].replace(/_/g, ' ')
  return { name: words.charAt(0).toUpperCase() + words.slice(1), source: tool[1], literal: false }
}

/** What an answered (or cancelled, or expired) approval ended in; null while it waits or for any other entry. */
export function settledAs(e: Entry): string | null {
  if (e.kind !== 'permission' || !e.data.status || e.data.status === 'pending') return null
  const options = e.data.options
  const chosen = options?.find((o) => o.optionId === e.data.selected)
  const chosenName = trimmedName(chosen?.name)
  if (chosen && hasDuplicateKind(chosen.kind, options) && chosenName) {
    return `${e.data.status}:${chosen.kind}:${chosenName}`
  }
  return `${e.data.status}:${chosen?.kind ?? ''}`
}

/**
 * The symbol of a request, so one tells from the next down a chat: by the tool's kind, and for a tool of no kind (an MCP
 * server's) by the verb its name starts with.
 */
export function requestIcon(toolKind?: string, title?: string): string {
  const byKind: Record<string, string> = { execute: 'terminal', edit: 'pencil', delete: 'trash', move: 'pencil', fetch: 'globe', read: 'doc.text', search: 'magnifyingglass' }
  if (toolKind && byKind[toolKind]) return byKind[toolKind]
  const verb = /^(.+)-(\w+): \2$/.exec(title?.trim() ?? '')?.[2].split('_')[0]
  const byVerb: Record<string, string> = { read: 'doc.text', get: 'doc.text', search: 'magnifyingglass', find: 'magnifyingglass', list: 'list.bullet', write: 'pencil', edit: 'pencil', update: 'pencil', create: 'pencil', add: 'pencil', save: 'pencil', delete: 'trash', remove: 'trash', fetch: 'globe', open: 'globe', run: 'terminal' }
  return (verb && byVerb[verb]) || 'terminal'
}

/** A request's detail as worth showing: without the Markdown fence agents wrap it in, and nothing when it is empty. */
export function requestDetail(text?: string): string | null {
  const bare = (text ?? '').trim().replace(/^```[\w-]*\n([\s\S]*?)\n?```$/, '$1').trim()
  return bare === '' || bare === '{}' || bare === '[]' ? null : bare
}

/** A request's arguments as name and value rows, when its detail is a JSON object; null otherwise. */
export function requestParams(detail: string | null): [string, string][] | null {
  if (!detail?.startsWith('{')) return null
  try {
    const value: unknown = JSON.parse(detail)
    if (!value || typeof value !== 'object' || Array.isArray(value)) return null
    const rows = Object.entries(value).map(([k, v]): [string, string] => [k, typeof v === 'string' ? v : JSON.stringify(v)])
    return rows.length ? rows : null
  } catch {
    return null
  }
}
