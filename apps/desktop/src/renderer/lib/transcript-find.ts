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

/** The ancestors that scroll vertically, innermost first (a card with its own scroll sits inside the transcript's). */
function scrollers(from: Element | null): Element[] {
  const found: Element[] = []
  for (let el = from; el; el = el.parentElement) {
    const overflow = getComputedStyle(el).overflowY
    if ((overflow === 'auto' || overflow === 'scroll') && el.scrollHeight > el.clientHeight) found.push(el)
  }
  return found
}

/** Scrolls the match itself (not the block it sits in: a long message or code block holds many) to the middle, in every box that scrolls around it. */
export function reveal(range: Range) {
  const parent = range.startContainer.parentElement
  const views = scrollers(parent)
  if (views.length === 0 || (range.getBoundingClientRect().width === 0 && range.getBoundingClientRect().height === 0)) {
    return void parent?.scrollIntoView({ block: 'center' })
  }
  for (const view of views) {
    const rect = range.getBoundingClientRect()
    const box = view.getBoundingClientRect()
    view.scrollBy({ top: rect.top - box.top - (view.clientHeight - rect.height) / 2 })
  }
}
