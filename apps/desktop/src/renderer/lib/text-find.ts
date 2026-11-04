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

/** Where `query` begins in `text`, ignoring case; overlapping matches don't count twice. */
export function matchStarts(text: string, query: string): number[] {
  const needle = query.toLocaleLowerCase()
  const hay = text.toLocaleLowerCase()
  // Lowercasing can change a text's length (some scripts); positions then wouldn't line up.
  if (!needle || hay.length !== text.length) return []
  const found: number[] = []
  for (let at = hay.indexOf(needle); at !== -1; at = hay.indexOf(needle, at + needle.length)) found.push(at)
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
  const length = query.length
  return matchStarts(joined.text, query).flatMap((at) => {
    const from = locate(joined, runs, at, false)
    const to = locate(joined, runs, at + length, true)
    return from && to ? [{ from, to }] : []
  })
}
