// Bare web addresses in message text (`https://…`, `http://…`, `www.…`) become links, found by the
// same rule as the iPhone, Android and terminal: the address runs to a space, quote, angle
// bracket, backtick, star or CJK punctuation, without the sentence's closing punctuation or an
// unmatched closing bracket.

/** Where a bare address may start: its prefix, not inside a word. */
export const urlStart = /(?<![\p{L}\p{N}_])(?:https?:\/\/|www\.)/iu

const stop = /[\s<>"`*　-〿＀-￯]/u
const trailing = '.,:;!?\'"*_~'

/** The length of the address at the start of `text` (which begins with its prefix), 0 for none. */
export function urlLength(text: string): number {
  const prefix = /^(?:https?:\/\/|www\.)/i.exec(text)?.[0]
  if (!prefix) return 0
  const end = text.slice(prefix.length).search(stop)
  let length = end < 0 ? text.length : prefix.length + end
  // Bracket counts are kept while trimming, so a long run of `)` costs one pass.
  const open = { ')': count(text, '(', length), ']': count(text, '[', length) }
  const close = { ')': count(text, ')', length), ']': count(text, ']', length) }
  for (; length > 0; length--) {
    const last = text[length - 1]!
    if (last === ')' || last === ']') {
      if (close[last] <= open[last]) break
      close[last]--
    } else if (!trailing.includes(last)) break
  }
  return /[\p{L}\p{N}]/u.test(text.slice(prefix.length, length)) ? length : 0
}

function count(text: string, char: string, length: number): number {
  let n = 0
  for (let i = 0; i < length; i++) if (text[i] === char) n++
  return n
}

/** What a found address opens: `www.` addresses over https, and the scheme in lowercase. */
export function urlHref(url: string): string {
  if (/^www\./i.test(url)) return `https://${url}`
  const colon = url.indexOf(':')
  return url.slice(0, colon).toLowerCase() + url.slice(colon)
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
