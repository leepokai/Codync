/** The text of a conversation as runs (one per text node), searched as one string so a match can cross a bold word or a link. */
export interface TextRun {
  text: string
  /** The block (paragraph, list item, message…) the run sits in: a match never crosses from one block to the next. */
  block: unknown
}

export interface JoinedRuns {
  text: string
  /** Where each run starts in `text`. */
  starts: number[]
}

/** Different blocks are kept apart by a line break, so a search can't match the end of one message plus the start of the next. */
const BREAK = '\n'

export function joinRuns(runs: TextRun[]): JoinedRuns {
  let text = ''
  const starts: number[] = []
  runs.forEach((run, i) => {
    if (i > 0 && run.block !== runs[i - 1]?.block) text += BREAK
    starts.push(text.length)
    text += run.text
  })
  return { text, starts }
}

/**
 * A string for comparing without case. It goes character by character, for the query too, so both sides fold the
 * same way: a Greek capital sigma lowercases to a final `ς` at the end of a whole word but not on its own, and
 * the two sigmas are one letter here.
 */
function fold(text: string): string {
  let out = ''
  for (const char of text) out += char.toLocaleLowerCase().replace(/ς/g, 'σ')
  return out
}

export interface Match {
  start: number
  end: number
}

/**
 * Where `query` is in `text`, ignoring case; overlapping matches don't count twice. Each character is lowercased on its
 * own, so one that changes length (`İ` becomes two) shifts only itself, and the positions still point into `text`.
 */
export function matchRanges(text: string, query: string): Match[] {
  const needle = fold(query)
  if (!needle) return []
  let hay = ''
  /** For each unit of `hay`, where its character starts in `text` and ends. */
  const origin: Match[] = []
  for (let at = 0; at < text.length; ) {
    const char = String.fromCodePoint(text.codePointAt(at)!)
    const lower = fold(char)
    for (let i = 0; i < lower.length; i++) origin.push({ start: at, end: at + char.length })
    hay += lower
    at += char.length
  }
  const found: Match[] = []
  for (let at = hay.indexOf(needle); at !== -1; at = hay.indexOf(needle, at + needle.length)) {
    found.push({ start: origin[at]!.start, end: origin[at + needle.length - 1]!.end })
  }
  return found
}

export interface RunPoint {
  run: number
  offset: number
}

/** The run and offset in it for a position in the joined text; a position on a run's end belongs to that run when `end`. */
export function locate(joined: JoinedRuns, runs: TextRun[], position: number, end: boolean): RunPoint | null {
  for (let run = joined.starts.length - 1; run >= 0; run--) {
    const start = joined.starts[run]!
    const length = runs[run]!.text.length
    const inside = end ? position > start && position <= start + length : position >= start && position < start + length
    if (inside) return { run, offset: position - start }
  }
  return null
}

export interface MatchSpan {
  from: RunPoint
  to: RunPoint
}

/** Every match of `query` in the runs, as where it starts and ends. */
export function findSpans(runs: TextRun[], query: string): MatchSpan[] {
  const joined = joinRuns(runs)
  return matchRanges(joined.text, query).flatMap(({ start, end }) => {
    const from = locate(joined, runs, start, false)
    const to = locate(joined, runs, end, true)
    return from && to ? [{ from, to }] : []
  })
}
