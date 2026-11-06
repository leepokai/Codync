/**
 * Reads the folder of another agent (OpenClaw, Hermes, or any folder with an AGENTS.md) into a plan a person reviews
 * before anything is created: instructions, memory and skills, each ticked or not. Pure: it works on the files the main
 * process read, so it is tested without a disk.
 *
 * Ported in design from Rakazo's `packages/core/src/agent-import/` (Apache-2.0, github.com/Rakazo): the file lists of
 * each format, the credential shapes, the Hermes `§` memory separator and bundled-skill manifest, and OpenClaw's
 * identity parsing. The code is Codync's and is not a copy; changed from the original: one small module, no archive
 * reader, no Rakazo contracts. See THIRD-PARTY-NOTICES.md.
 *
 * Rule that matters: **a credential never travels.** A file that is a settings or secret file is not read; text that looks
 * like a key is shown as omitted and never chosen; and the host refuses it again if it is sent anyway.
 */

export type ImportSource = 'openclaw' | 'hermes' | 'agents'

export type SkippedReason = 'secret' | 'runtime' | 'builtin' | 'default' | 'empty' | 'unreadable'

export interface ImportItem {
  /** Stable in the plan: the path it came from. */
  id: string
  title: string
  text: string
  /** Ticked when the plan is made; a credential-like or stock item is not (and a credential-like one cannot be). */
  selected: boolean
  /** The text has the shape of a key or token. It is shown as omitted and never sent. */
  credential: boolean
  /** A template the agent shipped with and the person never changed. */
  stock: boolean
}

export interface ImportSkill extends ImportItem {
  name: string
  description: string
}

export interface ImportPlan {
  source: ImportSource
  name: string
  description: string
  instructions: ImportItem[]
  memory: ImportItem[]
  skills: ImportSkill[]
  skipped: { path: string; reason: SkippedReason }[]
}

/** The longest file read, the most files and the most text in one import (the main process enforces them too). */
export const LIMITS = { file: 256 * 1024, files: 300, total: 4 * 1024 * 1024 } as const

// MARK: credentials

const SHAPES: { pattern: RegExp; random?: boolean }[] = [
  { pattern: /(?<![\w-])sk-(?:ant-(?:api\d\d-)?|or-v1-|proj-)?([A-Za-z0-9_-]{20,})/g, random: true },
  { pattern: /\bgh[pousr]_[A-Za-z0-9]{36,}/g },
  { pattern: /\bgithub_pat_[A-Za-z0-9_]{22,}/g },
  { pattern: /\bxox[abposr]-[A-Za-z0-9-]{10,}/g },
  { pattern: /\b(?:AKIA|ASIA)[0-9A-Z]{16}\b/g },
  { pattern: /\bAIza[0-9A-Za-z_-]{35}(?![0-9A-Za-z_-])/g },
  { pattern: /\bglpat-[A-Za-z0-9_-]{20,}/g },
  { pattern: /\bhf_[A-Za-z0-9]{30,}/g },
  { pattern: /\b[rs]k_live_[A-Za-z0-9]{20,}/g },
  { pattern: /-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----/g },
  { pattern: /\bbearer\s+([A-Za-z0-9._~+/-]{24,}=*)/gi, random: true },
  { pattern: /\b\d{6,}:[A-Za-z0-9_-]{30,}\b/g },
]

const looksRandom = (value: string) => /[A-Za-z]/.test(value) && /\d/.test(value) && new Set(value).size > 4

/** Whether text has the shape of a well-known key or token. Never returns the value. */
export function looksLikeCredential(text: string): boolean {
  for (const { pattern, random } of SHAPES) {
    pattern.lastIndex = 0
    for (const match of text.matchAll(pattern)) {
      if (!random || looksRandom(match[1] ?? match[0])) return true
    }
  }
  return false
}

/** Files that hold settings or secrets: never read, whatever they are named inside. */
const SECRET_FILE = /(^|\/)(\.env(\..*)?|auth\.json|credentials(\.json)?|secrets?(\.json|\.ya?ml)?|openclaw\.json|config\.ya?ml|.*\.(pem|key|p12|pfx))$/i

export const isSecretFile = (path: string) => SECRET_FILE.test(path)

// MARK: the formats

const OPENCLAW_INSTRUCTIONS = ['SOUL.md', 'AGENTS.md'] as const
const OPENCLAW_MEMORY = ['MEMORY.md', 'USER.md'] as const
const OPENCLAW_RUNTIME = ['TOOLS.md', 'HEARTBEAT.md', 'BOOTSTRAP.md', 'BOOT.md']
const HERMES_INSTRUCTIONS = ['SOUL.md', 'AGENTS.md', 'HERMES.md', '.hermes.md'] as const
const HERMES_MEMORY = ['memories/MEMORY.md', 'memories/USER.md'] as const
const BUNDLED_MANIFEST = 'skills/.bundled_manifest'
const DAILY_MEMORY = /^memory\/[^/]+\.md$/
const SKILL_FILE = /^skills\/(?:[^/]+\/)*SKILL\.md$/

/** The paths worth reading, so the main process sends nothing else. */
export function isCandidate(path: string): boolean {
  if (isSecretFile(path)) return false
  return (
    [...OPENCLAW_INSTRUCTIONS, ...OPENCLAW_MEMORY, ...OPENCLAW_RUNTIME, ...HERMES_INSTRUCTIONS, ...HERMES_MEMORY, 'IDENTITY.md', BUNDLED_MANIFEST].includes(path) ||
    DAILY_MEMORY.test(path) ||
    SKILL_FILE.test(path)
  )
}

/** OpenClaw or Hermes by what only they keep; any other folder with an AGENTS.md is a plain agent document. */
export function detectSource(paths: readonly string[]): ImportSource | null {
  const set = new Set(paths)
  if (set.has('IDENTITY.md') || set.has('SOUL.md') && !set.has('HERMES.md')) {
    return paths.some((p) => p.startsWith('memories/') || p === 'HERMES.md' || p === '.hermes.md') ? 'hermes' : 'openclaw'
  }
  if (paths.some((p) => p === 'HERMES.md' || p === '.hermes.md' || p.startsWith('memories/') || p === BUNDLED_MANIFEST)) return 'hermes'
  if (set.has('AGENTS.md') || paths.some((p) => SKILL_FILE.test(p))) return 'agents'
  return null
}

function item(id: string, title: string, text: string, stock = false): ImportItem {
  const credential = looksLikeCredential(text)
  return { id, title, text, credential, stock, selected: !credential && !stock }
}

/** Hermes separates memory entries with a line holding only `§`; one paragraph each. */
export function hermesEntries(content: string): string {
  const entries = content.replace(/\r\n?/g, '\n').split('\n').reduce<string[][]>((all, line) => {
    if (line.trim() === '§') all.push([])
    else all[all.length - 1]!.push(line)
    return all
  }, [[]])
  const kept = entries.map((lines) => lines.join('\n').trim()).filter(Boolean)
  return kept.length ? `${kept.join('\n\n')}\n` : ''
}

/** `name:` and `description:` of a SKILL.md's front matter, and the body after it. */
export function parseSkill(content: string): { name: string; description: string; body: string } {
  const text = content.replace(/\r\n?/g, '\n')
  const match = /^---\n([\s\S]*?)\n---\n?([\s\S]*)$/.exec(text)
  const front = match?.[1] ?? ''
  const field = (key: string) => {
    const line = new RegExp(`^${key}:\\s*(.+)$`, 'mi').exec(front)?.[1]?.trim() ?? ''
    return line.replace(/^["']|["']$/g, '')
  }
  const body = (match ? match[2] : text) ?? ''
  const heading = /^#\s+(.+)$/m.exec(body)?.[1]?.trim() ?? ''
  return { name: field('name') || heading, description: field('description'), body: body.trim() }
}

/** OpenClaw's IDENTITY.md: `- Name: …` lines; its template's hints count as empty. */
export function parseIdentity(content: string): { name: string; description: string } {
  const hints = /^(not set yet|pick something you like|\(.*\)|.*\?.*\?)$/i
  const fields = new Map<string, string>()
  for (const line of content.replace(/\r\n?/g, '\n').split('\n')) {
    const cleaned = line.trim().replace(/^-\s*/, '').replace(/[*_`]/g, '')
    const colon = cleaned.indexOf(':')
    if (colon <= 0) continue
    const value = cleaned.slice(colon + 1).trim()
    if (value && !hints.test(value)) fields.set(cleaned.slice(0, colon).trim().toLowerCase(), value)
  }
  const description = ['theme', 'creature', 'vibe'].map((k) => fields.get(k)).find(Boolean) ?? ''
  return { name: fields.get('name') ?? '', description }
}

const bundledNames = (manifest: string) =>
  new Set(manifest.split(/\r?\n/).map((line) => line.split(':')[0]!.trim().toLowerCase()).filter(Boolean))

/** The plan for the files of one agent folder (`path` relative to its home → text; `secretPaths`: files left unread). Null when it is not an agent's. */
export function planImport(files: Readonly<Record<string, string>>, secretPaths: readonly string[] = []): ImportPlan | null {
  const paths = Object.keys(files).filter(isCandidate)
  const source = detectSource(paths)
  if (!source) return null
  const text = (path: string) => files[path] ?? ''
  const skipped: ImportPlan['skipped'] = []
  const identity = source === 'openclaw' ? parseIdentity(text('IDENTITY.md')) : { name: '', description: '' }

  const instructionFiles = source === 'hermes' ? HERMES_INSTRUCTIONS : OPENCLAW_INSTRUCTIONS
  const instructions = instructionFiles.filter((p) => text(p).trim()).map((p) => item(p, p, text(p).trim()))

  const memory: ImportItem[] = []
  if (source === 'hermes') {
    for (const p of HERMES_MEMORY) {
      const body = hermesEntries(text(p))
      if (body) memory.push(item(p, p, body.trim()))
    }
  } else {
    for (const p of [...OPENCLAW_MEMORY, ...paths.filter((q) => DAILY_MEMORY.test(q)).sort()]) {
      if (text(p).trim()) memory.push(item(p, p, text(p).trim()))
    }
  }
  if (source === 'openclaw') {
    for (const p of OPENCLAW_RUNTIME) if (files[p] !== undefined) skipped.push({ path: p, reason: 'runtime' })
  }

  const bundled = bundledNames(text(BUNDLED_MANIFEST))
  const skills: ImportSkill[] = []
  for (const p of paths.filter((q) => SKILL_FILE.test(q)).sort()) {
    const { name, description, body } = parseSkill(text(p))
    const folder = p.split('/').at(-2) ?? ''
    if (!body) { skipped.push({ path: p, reason: 'empty' }); continue }
    if (source === 'hermes' && (bundled.has(name.toLowerCase()) || bundled.has(folder.toLowerCase()))) {
      skipped.push({ path: p, reason: 'builtin' })
      continue
    }
    skills.push({ ...item(p, name || folder, body), name: name || folder, description })
  }
  // Settings and secret files are named, never read: the main process lists them apart from the files it read.
  for (const path of secretPaths) skipped.push({ path, reason: 'secret' })

  const fallback = source === 'hermes' ? 'Hermes' : source === 'openclaw' ? 'OpenClaw' : 'Imported agent'
  return {
    source,
    name: identity.name || fallback,
    description: identity.description,
    instructions,
    memory,
    skills,
    skipped: skipped.sort((a, b) => a.path.localeCompare(b.path)),
  }
}

/** What the person has ticked, ready for the host. A credential-like item is never included, ticked or not: it is shown as omitted. */
export function chosen(plan: ImportPlan, ticked: ReadonlySet<string>) {
  const keep = <T extends ImportItem>(items: T[]) => items.filter((i) => ticked.has(i.id) && !i.credential)
  return {
    instructions: keep(plan.instructions),
    memory: keep(plan.memory),
    skills: keep(plan.skills),
  }
}

/** The ticks a plan starts with. */
export const initialTicks = (plan: ImportPlan): Set<string> =>
  new Set([...plan.instructions, ...plan.memory, ...plan.skills].filter((i) => i.selected).map((i) => i.id))

/** The bot's description: its instruction files, one after the other. */
export const instructionText = (items: ImportItem[]) => items.map((i) => i.text).join('\n\n')

/** Memory as one fact per paragraph or bullet, for the host. */
export function memoryFacts(items: ImportItem[]): string[] {
  const facts: string[] = []
  for (const it of items) {
    for (const block of it.text.split(/\n{2,}/)) {
      const line = block.trim().replace(/\n+/g, ' ')
      if (line) facts.push(line.replace(/^[-*]\s+/, ''))
    }
  }
  return facts
}

export const skippedText: Record<SkippedReason, string> = {
  secret: 'Settings or secrets: never read',
  runtime: 'Only meaningful to that agent’s runtime',
  builtin: 'Installed by the agent itself',
  default: 'An unchanged template',
  empty: 'Nothing in it',
  unreadable: 'Could not be read',
}
