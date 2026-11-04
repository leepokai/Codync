/** "What I know about you": each note of a bot's memory as a belief (`beliefs`), and the words every screen uses. */

export type BeliefKind = 'user' | 'feedback' | 'project' | 'reference'
export type BeliefConfidence = 'high' | 'medium' | 'low'

export interface BeliefVersion {
  id: number
  /** Null: the note was forgotten in this version. */
  content: string | null
  cause: string
  at: number
}

export interface Belief {
  file: string
  shared: boolean
  description: string | null
  kind: BeliefKind
  confidence: BeliefConfidence
  /** `YYYY-MM-DD`, or null when nobody confirmed it yet. */
  confirmed: string | null
  updatedAt: number
  tokens: number
  versions: BeliefVersion[]
}

export const KINDS: { id: BeliefKind; label: string }[] = [
  { id: 'user', label: 'About you' },
  { id: 'feedback', label: 'How to work' },
  { id: 'project', label: 'Projects' },
  { id: 'reference', label: 'References' },
]

export const CONFIDENCES: { id: BeliefConfidence; label: string }[] = [
  { id: 'high', label: 'Sure' },
  { id: 'medium', label: 'Probably' },
  { id: 'low', label: 'Unsure' },
]

export const kindLabel = (kind: BeliefKind) => KINDS.find((k) => k.id === kind)?.label ?? 'About you'
export const confidenceLabel = (confidence: BeliefConfidence) => CONFIDENCES.find((c) => c.id === confidence)?.label ?? 'Probably'

/** Filters as the screen holds them: `null` is "all". */
export interface BeliefFilter {
  kind: BeliefKind | null
  confidence: BeliefConfidence | null
}

export function filterBeliefs(beliefs: readonly Belief[], filter: BeliefFilter): Belief[] {
  return beliefs.filter((b) => (filter.kind === null || b.kind === filter.kind) && (filter.confidence === null || b.confidence === filter.confidence))
}

/** When it was last confirmed, for the row. */
export const confirmedText = (confirmed: string | null) => (confirmed ? `Confirmed ${confirmed}` : 'Never confirmed')

/** The unsure first, then the oldest unconfirmed: what is worth asking about comes to the top. */
export function byAttention(beliefs: readonly Belief[]): Belief[] {
  const rank = { low: 0, medium: 1, high: 2 } as const
  return [...beliefs].sort((a, b) => rank[a.confidence] - rank[b.confidence] || (a.confirmed ?? '').localeCompare(b.confirmed ?? '') || a.file.localeCompare(b.file))
}

const CAUSES: Record<string, string> = {
  edit: 'Changed', delete: 'Forgotten', restore: 'Brought back', undo: 'Undone', dream: 'Tidied while dreaming',
}

/** One line per version, newest first, for the history; a version can be brought back unless it is the forgetting itself. */
export function historyRows(belief: Belief): { id: number; label: string; at: number; restorable: boolean }[] {
  return belief.versions.map((v, i) => ({
    id: v.id,
    label: CAUSES[v.cause] ?? 'Changed',
    at: v.at,
    restorable: v.content !== null && i > 0,
  }))
}

/** The note's text without its header lines, for a preview. */
export function bodyPreview(content: string, max = 160): string {
  const body = content.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n?/, '').trim().replace(/\s+/g, ' ')
  return body.length > max ? `${body.slice(0, max - 1)}…` : body
}
