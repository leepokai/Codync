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

export function inlineParts(text: string): InlinePart[] {
  const out: InlinePart[] = []
  let rest = text
  while (rest) {
    const m = pattern.exec(rest)
    if (!m) {
      out.push({ kind: 'text', text: rest })
      break
    }
    if (m.index > 0) out.push({ kind: 'text', text: rest.slice(0, m.index) })
    if (m[1] !== undefined) out.push({ kind: 'code', text: m[1].slice(1, -1) })
    else if (m[2] !== undefined) out.push({ kind: 'link', url: m[3]!, children: inlineParts(m[2]) })
    else if (m[4] !== undefined || m[5] !== undefined) {
      out.push({ kind: 'strong', children: [{ kind: 'em', children: inlineParts(m[4] ?? m[5]!) }] })
    } else if (m[6] !== undefined || m[7] !== undefined) {
      out.push({ kind: 'strong', children: inlineParts(m[6] ?? m[7]!) })
    } else if (m[8] !== undefined) out.push({ kind: 'strike', children: inlineParts(m[8]) })
    else out.push({ kind: 'em', children: inlineParts(m[9] ?? m[10]!) })
    rest = rest.slice(m.index + m[0].length)
  }
  return out
}
