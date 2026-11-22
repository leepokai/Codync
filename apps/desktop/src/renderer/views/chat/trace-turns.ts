import type { Entry, Subagent } from '../../../shared/models.ts'

/** A bot's entries grouped by turn, oldest turn first, each turn's entries in their order. */
export function traceTurns(entries: Entry[]): [number, Entry[]][] {
  const turns = new Map<number, Entry[]>()
  for (const e of entries) {
    const turn = turns.get(e.turn)
    if (turn) turn.push(e)
    else turns.set(e.turn, [e])
  }
  return [...turns.entries()].sort((a, b) => a[0] - b[0])
}

/** The turn running now while the bot works in this chat; a queued message already holds the next turn. */
export function runningTurn(entries: Entry[], working: boolean): number | undefined {
  if (!working) return undefined
  return entries.reduce((t, e) => (e.data.status === 'queued' ? t : Math.max(t, e.turn)), -1)
}

/** What the running turn has done so far, in order: the steps the working indicator unfolds. */
export function workingSteps(entries: Entry[]): Entry[] {
  const turn = runningTurn(entries, true)
  return entries.filter((e) => e.turn === turn && isWorkingStep(e))
}

/** Thinking, narration, a tool call or a plan; never the bot's bookkeeping, nor a call the host didn't flag. */
function isWorkingStep(e: Entry): boolean {
  switch (e.kind) {
    case 'thought':
      return !!e.data.text?.trim()
    case 'agent':
      return !e.data.final && !isPass(e.data.text)
    case 'tool':
      return e.data.bookkeeping === false
    case 'plan':
      return true
    default:
      return false
  }
}

/** Steps with each subagent's steps right under the call that launched it (`parent`, in the same turn), `depth`
 * deeper. A step whose launch isn't among them stays where it is. */
export function nestedSteps(steps: Entry[]): { entry: Entry; depth: number }[] {
  // A call is known by its turn and id: a launch is looked for in its own turn only.
  const key = (e: Entry, id?: string) => `${e.turn}:${id}`
  const ids = new Set(steps.filter((e) => e.data.toolCallId).map((e) => key(e, e.data.toolCallId)))
  const launched = (e: Entry) => !!e.data.parent && e.data.parent !== e.data.toolCallId && ids.has(key(e, e.data.parent))
  const children = new Map<string, Entry[]>()
  for (const e of steps) {
    if (!launched(e)) continue
    const list = children.get(key(e, e.data.parent))
    if (list) list.push(e)
    else children.set(key(e, e.data.parent), [e])
  }
  const out: { entry: Entry; depth: number }[] = []
  const visit = (entry: Entry, depth: number) => {
    out.push({ entry, depth })
    for (const child of children.get(key(entry, entry.data.toolCallId)) ?? []) visit(child, depth + 1)
  }
  for (const e of steps) if (!launched(e)) visit(e, 0)
  return out
}

/** A subagent's launch, quietly: its kind, then the tokens it used and how long it ran once it finished, or that it
 * runs in the background. Nothing while a foreground one runs. */
export function subagentLine(s: Subagent | undefined, tr: (key: string) => string): string | null {
  if (!s || (!s.background && s.tokens === undefined)) return null
  const parts = s.kind ? [s.kind] : []
  if (s.background) parts.push(tr('in background'))
  if (s.tokens !== undefined) parts.push(tr('{tokens} tokens').replaceAll('{tokens}', () => compactCount(s.tokens!)))
  if (s.durationMs !== undefined) parts.push(duration(s.durationMs, tr))
  return parts.join(' · ')
}

/** 812, 42.3k, 1.2M. */
function compactCount(n: number): string {
  if (n < 1000) return String(n)
  const [div, unit] = n < 1_000_000 ? [1000, 'k'] : [1_000_000, 'M']
  const tenths = Math.round(n / (div / 10))
  return `${tenths < 1000 ? tenths / 10 : Math.round(n / div)}${unit}`
}

/** 6 s, 2 min. */
function duration(ms: number, tr: (key: string) => string): string {
  const s = Math.max(1, Math.round(ms / 1000))
  return s < 60 ? tr('{count} s').replaceAll('{count}', () => String(s)) : tr('{count} min').replaceAll('{count}', () => String(Math.round(s / 60)))
}

/** No text, or a pass the host keeps out of the chat (`is_pass` in host/src/chat/group.rs). */
function isPass(text = ''): boolean {
  const t = text.trim().replace(/^[".*]+|[".*]+$/g, '').toLowerCase()
  return t === '' || t === '(pass)' || t === 'pass'
}

/** Scrolled to (within a few pixels of) the end: new steps keep it there. */
export function scrolledToEnd(el: { scrollHeight: number; scrollTop: number; clientHeight: number }) {
  return el.scrollHeight - el.scrollTop - el.clientHeight < 8
}
