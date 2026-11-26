import { memo, type ReactNode } from 'react'
import { parseMarkdown, type Block } from './markdown-blocks.ts'
import { Hairline } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import './markdown.css'

// The Mac's Markdown for messages: the blocks from `markdown-blocks.ts`, with inline styling like
// `AttributedString(markdown:)`.

import { inlineParts, type InlinePart } from './markdown-inline.ts'

/** Inline Markdown: `code` (tinted), **bold**, *italic*, ~~strike~~, [links](url). */
export function inline(text: string, key = 'i'): ReactNode[] {
  return renderInlineParts(inlineParts(text), key)
}

function renderInlineParts(parts: InlinePart[], key: string): ReactNode[] {
  const out: ReactNode[] = []
  parts.forEach((part, i) => {
    const k = `${key}${i}`
    if (part.kind === 'text') out.push(part.text)
    else if (part.kind === 'code') out.push(<code key={k} className="md-code-span">{part.text}</code>)
    else if (part.kind === 'link') out.push(
      <a key={k} href={part.url} title={part.url} className="md-link" onClick={(e) => { e.preventDefault(); window.codync.app.openExternal(part.url) }}>
        {renderInlineParts(part.children, `${k}-`)}
      </a>,
    )
    else if (part.kind === 'strong') out.push(<strong key={k} className="md-strong">{renderInlineParts(part.children, `${k}-`)}</strong>)
    else if (part.kind === 'strike') out.push(<s key={k}>{renderInlineParts(part.children, `${k}-`)}</s>)
    else out.push(<em key={k}>{renderInlineParts(part.children, `${k}-`)}</em>)
  })
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
          {/* Each cell is its own block to find in: a match never runs from one cell into the next. */}
          <div className="md-table md-body" style={{ gridTemplateColumns: `repeat(${columns}, max-content)` }}>
            {cols.map((i) => (
              <Text key={`h${i}`} weight="semibold" data-find-block>{inline(block.header[i] ?? '', `h${i}`)}</Text>
            ))}
            {block.rows.map((row, r) => [
              <Hairline key={`l${r}`} style={{ gridColumn: `1 / span ${columns}` }} />,
              ...cols.map((i) => <span key={`${r}-${i}`} data-find-block>{inline(row[i] ?? '', `${r}-${i}`)}</span>),
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
/** `findable`: a message's text, which find in the conversation searches. */
export const MarkdownText = memo(function MarkdownText({ text, findable = false }: { text: string; findable?: boolean }) {
  const blocks = parseMarkdown(text)
  return (
    <VStack gap={10} shrink className="markdown selectable" data-find-text={findable || undefined}>
      {blocks.map((b, i) => (
        <MarkdownBlock key={i} block={b} />
      ))}
    </VStack>
  )
})
