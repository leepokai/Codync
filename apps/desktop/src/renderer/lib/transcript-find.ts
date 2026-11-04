import { findSpans, type TextRun } from './text-find'

/** Finding text in the conversation on screen, drawn with the page's own highlights (`::highlight(find-match)`, `find-active`). */
const BLOCKS = 'p, li, pre, blockquote, h1, h2, h3, h4, h5, h6, tr, td, th, table, section, article, button, div'
const SKIPPED = 'script, style, [aria-hidden="true"]'

type VisibilityCheck = { checkVisibility?: (options?: { checkVisibilityCSS?: boolean }) => boolean }

/** The visible text nodes under `root`, in reading order, each with the block it sits in. */
function collect(root: HTMLElement): { runs: TextRun[]; nodes: Text[] } {
  const runs: TextRun[] = []
  const nodes: Text[] = []
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT)
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const text = node as Text
    const parent = text.parentElement
    if (!parent || !text.data || parent.closest(SKIPPED)) continue
    if ((parent as VisibilityCheck).checkVisibility?.({ checkVisibilityCSS: true }) === false) continue
    runs.push({ text: text.data, block: parent.closest(BLOCKS) ?? root })
    nodes.push(text)
  }
  return { runs, nodes }
}

/** Every match of `query` under `root`, as page ranges, top to bottom. */
export function findRanges(root: HTMLElement, query: string): Range[] {
  if (!query.trim()) return []
  const { runs, nodes } = collect(root)
  return findSpans(runs, query).map(({ from, to }) => {
    const range = document.createRange()
    range.setStart(nodes[from.run]!, from.offset)
    range.setEnd(nodes[to.run]!, to.offset)
    return range
  })
}

/** Highlights every match, and the one at `active` more strongly. */
export function paint(ranges: Range[], active: number) {
  const all = new Highlight(...ranges)
  const current = new Highlight(...(ranges[active] ? [ranges[active]] : []))
  current.priority = 1
  CSS.highlights.set('find-match', all)
  CSS.highlights.set('find-active', current)
}

export function clearPaint() {
  CSS.highlights.delete('find-match')
  CSS.highlights.delete('find-active')
}

/** Scrolls the match to the middle of the conversation. */
export function reveal(range: Range) {
  range.startContainer.parentElement?.scrollIntoView({ block: 'center' })
}
