// Bare web addresses in message text (`https://…`, `http://…`, `www.…`) become links, found by the
// same rule as the iPhone, Android and terminal: the address runs to a space, quote, angle
// bracket, backtick or CJK punctuation, without the sentence's closing punctuation or an
// unmatched closing bracket.

/** Where a bare address may start: its prefix, not inside a word. */
export const urlStart = /(?<![\p{L}\p{N}_])(?:https?:\/\/|www\.)/iu

const stop = /[\s<>"`　-〿＀-￯]/u
const trailing = '.,:;!?\'"*_~'

/** The length of the address at the start of `text` (which begins with its prefix), 0 for none. */
export function urlLength(text: string): number {
  const prefix = /^(?:https?:\/\/|www\.)/i.exec(text)?.[0]
  if (!prefix) return 0
  const end = text.slice(prefix.length).search(stop)
  let url = end < 0 ? text : text.slice(0, prefix.length + end)
  for (;;) {
    const last = url.at(-1) ?? ''
    if (trailing.includes(last) || unmatched(url, last)) url = url.slice(0, -1)
    else break
  }
  return /[\p{L}\p{N}]/u.test(url.slice(prefix.length)) ? url.length : 0
}

function unmatched(url: string, last: string): boolean {
  const open = { ')': '(', ']': '[' }[last]
  return open !== undefined && url.split(last).length > url.split(open).length
}

/** What a found address opens: `www.` addresses over https. */
export function urlHref(url: string): string {
  return /^www\./i.test(url) ? `https://${url}` : url
}

export type TextPiece = string | { url: string; href: string }

/** Plain text split into text and the addresses in it. */
export function splitURLs(text: string): TextPiece[] {
  const out: TextPiece[] = []
  let rest = text
  for (;;) {
    const m = urlStart.exec(rest)
    const length = m ? urlLength(rest.slice(m.index)) : 0
    if (!m || !length) {
      if (m) {
        out.push(rest.slice(0, m.index + m[0].length))
        rest = rest.slice(m.index + m[0].length)
        continue
      }
      if (rest) out.push(rest)
      return out
    }
    if (m.index > 0) out.push(rest.slice(0, m.index))
    const url = rest.slice(m.index, m.index + length)
    out.push({ url, href: urlHref(url) })
    rest = rest.slice(m.index + length)
  }
}
