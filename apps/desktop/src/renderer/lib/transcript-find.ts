import { centeringOffset, offsetToShow } from './find-position'
import { findSpans, type TextRun } from './text-find'

/**
 * Finding text in the conversation on screen, drawn with the page's own highlights (`::highlight(find-match)`,
 * `find-active`). Views mark what the find treats specially: `data-find-block` on an element that holds text of its
 * own without being one of these tags (a table cell drawn as a span in a grid), `data-find-skip` on the transcript's
 * own text that isn't the conversation (Load earlier messages, an empty chat's hints).
 */
const BLOCKS = '[data-find-block], p, li, pre, blockquote, h1, h2, h3, h4, h5, h6, tr, td, th, table, section, article, button, div'
const SKIPPED = 'script, style, [aria-hidden="true"], [data-find-skip]'

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

const SCROLLING = new Set(['auto', 'scroll'])

/** A box around the match that scrolls, and on which axes. */
interface Scroller {
  el: Element
  x: boolean
  y: boolean
}

/** The ancestors that scroll, innermost first (a wide table or code block scrolls sideways inside the transcript). */
function scrollers(from: Element | null): Scroller[] {
  const found: Scroller[] = []
  for (let el = from; el; el = el.parentElement) {
    const style = getComputedStyle(el)
    const x = SCROLLING.has(style.overflowX) && el.scrollWidth > el.clientWidth
    const y = SCROLLING.has(style.overflowY) && el.scrollHeight > el.clientHeight
    if (x || y) found.push({ el, x, y })
  }
  return found
}

/**
 * Scrolls the match itself (not the block it sits in: a long message or code block holds many) into view in every box
 * that scrolls around it: to the middle top to bottom, and sideways only when it is out of sight.
 */
export function reveal(range: Range) {
  const parent = range.startContainer.parentElement
  const views = scrollers(parent)
  const first = range.getBoundingClientRect()
  if (views.length === 0 || (first.width === 0 && first.height === 0)) return void parent?.scrollIntoView({ block: 'center' })
  for (const { el, x, y } of views) {
    const rect = range.getBoundingClientRect()
    const box = el.getBoundingClientRect()
    el.scrollBy({
      top: y ? centeringOffset({ start: rect.top, size: rect.height }, { start: box.top + el.clientTop, size: el.clientHeight }) : 0,
      left: x ? offsetToShow({ start: rect.left, size: rect.width }, { start: box.left + el.clientLeft, size: el.clientWidth }) : 0,
    })
  }
}
