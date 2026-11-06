import { memo, type ReactNode } from 'react'
import { parseMarkdown, type Block } from './markdown-blocks.ts'
import { Hairline } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import './markdown.css'

// The Mac's Markdown for messages: the blocks from `markdown-blocks.ts`, with inline styling like
// `AttributedString(markdown:)`.

/** Inline Markdown: `code` (tinted), **bold**, *italic*, ~~strike~~, [links](url). */
export function inline(text: string, key = 'i'): ReactNode[] {
  const out: ReactNode[] = []
  // Code spans first: nothing inside them is styled.
  text.split(/(`[^`\n]+`)/).forEach((part, i) => {
    if (part.length > 2 && part.startsWith('`') && part.endsWith('`')) {
      out.push(
        <code key={`${key}${i}`} className="md-code-span">
          {part.slice(1, -1)}
        </code>,
      )
    } else if (part) out.push(...emphasis(part, `${key}${i}-`))
  })
  return out
}

const pattern = /\[([^\]]+)\]\(([^)\s]+)\)|\*\*([^*]+)\*\*|__([^_]+)__|~~([^~]+)~~|\*([^*\n]+)\*|(?<![\w])_([^_\n]+)_(?![\w])/
function emphasis(text: string, key: string): ReactNode[] {
  const out: ReactNode[] = []
  let rest = text
  let n = 0
  while (rest) {
    const m = pattern.exec(rest)
    if (!m) {
      out.push(rest)
      break
    }
    if (m.index > 0) out.push(rest.slice(0, m.index))
    const k = `${key}${n++}`
    if (m[1] !== undefined) {
      const url = m[2]!
      out.push(
        <a key={k} href={url} title={url} className="md-link" onClick={(e) => { e.preventDefault(); window.codync.app.openExternal(url) }}>
          {emphasis(m[1], `${k}-`)}
        </a>,
      )
    } else if (m[3] !== undefined || m[4] !== undefined) out.push(<strong key={k} className="md-strong">{emphasis(m[3] ?? m[4]!, `${k}-`)}</strong>)
    else if (m[5] !== undefined) out.push(<s key={k}>{emphasis(m[5], `${k}-`)}</s>)
    else out.push(<em key={k}>{emphasis(m[6] ?? m[7]!, `${k}-`)}</em>)
    rest = rest.slice(m.index + m[0].length)
  }
  return out
}

const MarkdownBlock = memo(function MarkdownBlock({ block }: { block: Block }) {
  switch (block.kind) {
    case 'paragraph':
      return <div className="md-text md-spaced md-body">{inline(block.text)}</div>
    case 'heading':
      return (
        <div className={`md-text md-spaced md-heading-${Math.min(block.level, 3)}`}>
          {inline(block.text)}
        </div>
      )
    case 'bullet':
      return (
        <HStack align="baseline" gap={6} className="md-spaced md-body" style={{ paddingLeft: block.depth * 16 }}>
          <Text tone="secondary" className="md-marker">{block.marker}</Text>
          <span className="md-text md-bullet-text">{inline(block.text)}</span>
        </HStack>
      )
    case 'rule':
      return <Hairline className="md-rule" />
    case 'table': {
      const columns = Math.max(block.header.length, ...block.rows.map((r) => r.length))
      const cols = Array.from({ length: columns }, (_, i) => i)
      return (
        <div className="md-table-scroll">
          <div className="md-table md-body" style={{ gridTemplateColumns: `repeat(${columns}, max-content)` }}>
            {cols.map((i) => (
              <Text key={`h${i}`} weight="semibold">{inline(block.header[i] ?? '', `h${i}`)}</Text>
            ))}
            {block.rows.map((row, r) => [
              <Hairline key={`l${r}`} style={{ gridColumn: `1 / span ${columns}` }} />,
              ...cols.map((i) => <span key={`${r}-${i}`}>{inline(row[i] ?? '', `${r}-${i}`)}</span>),
            ])}
          </div>
        </div>
      )
    }
    case 'quote':
      return <div className="md-text md-spaced md-body md-quote">{inline(block.text)}</div>
    case 'code':
      return (
        <div className="md-code">
          <pre className="md-code-text">{block.text}</pre>
        </div>
      )
  }
}, (a, b) => JSON.stringify(a.block) === JSON.stringify(b.block))

/** The desktop's Markdown for messages; a block that didn't change keeps its rendering. */
export const MarkdownText = memo(function MarkdownText({ text }: { text: string }) {
  const blocks = parseMarkdown(text)
  return (
    <VStack gap={10} shrink className="markdown selectable">
      {blocks.map((b, i) => (
        <MarkdownBlock key={i} block={b} />
      ))}
    </VStack>
  )
})
