// Inline Markdown as message text uses it: code spans stay atomic, emphasis can cross them.
// Pure parsing, tested on its own; Markdown.tsx turns these parts into React nodes.

export type InlinePart =
  | { kind: 'text'; text: string }
  | { kind: 'code'; text: string }
  | { kind: 'link'; url: string; children: InlinePart[] }
  | { kind: 'strong'; children: InlinePart[] }
  | { kind: 'strike'; children: InlinePart[] }
  | { kind: 'em'; children: InlinePart[] }

// A delimiter closes at the first exact marker; a single `*` or `_` inside a stronger run stays text.
const pattern =
  /(`[^`\n]+`)|\[([^\]]+)\]\(([^)\s]+)\)|\*\*\*((?:(?!\*\*\*)[\s\S])+?)\*\*\*|___((?:(?!___)[\s\S])+?)___|\*\*((?:(?!\*\*)[\s\S])+?)\*\*|__((?:(?!__)[\s\S])+?)__|~~((?:(?!~~)[\s\S])+?)~~|\*([^*\n]+)\*|(?<!\w)_([^_\n]+)_(?!\w)/

// Code spans are pulled out before emphasis is matched, so a delimiter inside one (`**a `x**y` b**`) can never
// close the run around it. Each is replaced by a lone sentinel the emphasis patterns cannot match.
const OPEN = '\u0000'
const CLOSE = '\u0001'

function maskCode(text: string, codes: string[]): string {
  return text.replace(/`([^`\n]+)`/g, (_, code: string) => {
    codes.push(code)
    return `${OPEN}${codes.length - 1}${CLOSE}`
  })
}

/** `text` with its sentinels turned into the code spans they stand for. */
function unmask(text: string, codes: string[]): InlinePart[] {
  const out: InlinePart[] = []
  const pattern = new RegExp(`${OPEN}(\\d+)${CLOSE}`, 'g')
  let rest = text
  let m: RegExpExecArray | null
  while ((m = pattern.exec(rest))) {
    if (m.index > 0) out.push({ kind: 'text', text: rest.slice(0, m.index) })
    out.push({ kind: 'code', text: codes[Number(m[1])]! })
    rest = rest.slice(m.index + m[0].length)
    pattern.lastIndex = 0
  }
  if (rest) out.push({ kind: 'text', text: rest })
  return out
}

export function inlineParts(text: string): InlinePart[] {
  const codes: string[] = []
  const masked = maskCode(text, codes)
  const out: InlinePart[] = []
  const pushText = (value: string) => {
    for (const part of unmask(value, codes)) out.push(part)
  }
  let rest = masked
  while (rest) {
    const m = pattern.exec(rest)
    if (!m) {
      pushText(rest)
      break
    }
    if (m.index > 0) pushText(rest.slice(0, m.index))
    if (m[1] !== undefined) pushText(m[1])
    else if (m[2] !== undefined) out.push({ kind: 'link', url: m[3]!, children: inlineParts(unmaskToText(m[2], codes)) })
    else if (m[4] !== undefined || m[5] !== undefined) {
      out.push({ kind: 'strong', children: [{ kind: 'em', children: inlineParts(unmaskToText(m[4] ?? m[5]!, codes)) }] })
    } else if (m[6] !== undefined || m[7] !== undefined) {
      out.push({ kind: 'strong', children: inlineParts(unmaskToText(m[6] ?? m[7]!, codes)) })
    } else if (m[8] !== undefined) out.push({ kind: 'strike', children: inlineParts(unmaskToText(m[8], codes)) })
    else out.push({ kind: 'em', children: inlineParts(unmaskToText(m[9] ?? m[10]!, codes)) })
    rest = rest.slice(m.index + m[0].length)
  }
  return out
}

/** Children are parsed again, so their sentinels must become code spans back before the inner call. */
function unmaskToText(value: string, codes: string[]): string {
  return unmask(value, codes)
    .map((part) => (part.kind === 'code' ? `\`${part.text}\`` : part.kind === 'text' ? part.text : ''))
    .join('')
}
