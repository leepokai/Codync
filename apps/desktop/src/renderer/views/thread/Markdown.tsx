import { memo, useState, type ReactNode } from 'react'
import { parseMarkdown, type Block } from './markdown-blocks.ts'
import { Hairline } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { highlightSpans } from './code-highlight.ts'
import { looksLikeDiff, parseDiff, type DiffRow } from './diff-rows.ts'
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
      return <CodeBlock text={block.text} language={block.language} />
  }
}, (a, b) => JSON.stringify(a.block) === JSON.stringify(b.block))

/**
 * A fenced code block of a reply (idea 25): the language, a per-block wrap switch and a copy
 * button above the code. Copy puts the literal text on the clipboard, spaces and line breaks
 * intact. Diff blocks render through [DiffBlock]; other languages are highlighted locally.
 */
function CodeBlock({ text, language }: { text: string; language: string }) {
  const diff = looksLikeDiff(language, text) ? parseDiff(text) : null
  if (diff) return <DiffBlock rows={diff} original={text} language={language} />
  return <CodeFrame label={language.trim() || 'Code'} copyText={text} />
}

function CodeFrame({ label, copyText }: { label: string; copyText: string }) {
  const [wrap, setWrap] = useState(false)
  const [copied, setCopied] = useState(false)
  const copy = () => {
    void window.codync.app.copy(copyText)
    setCopied(true)
    window.setTimeout(() => setCopied(false), 1500)
  }
  return (
    <div className="md-code">
      <div className="md-code-header">
        <Text size="caption" mono tone="secondary" className="md-code-label">{label}</Text>
        <button className="md-code-action" role="switch" aria-checked={wrap} aria-label="Wrap lines" title="Wrap lines"
          onClick={() => setWrap((w) => !w)}>
          <span className={wrap ? 'md-code-action-text on' : 'md-code-action-text'}>Wrap</span>
        </button>
        <button className="md-code-action" aria-label={copied ? 'Copied' : 'Copy code'} title={copied ? 'Copied' : 'Copy code'} onClick={copy}>
          <Icon name={copied ? 'checkmark' : 'doc.on.doc'} size={12} weight="medium" />
        </button>
      </div>
      <pre className={wrap ? 'md-code-text wrap' : 'md-code-text'}>{highlighted(copyText, label)}</pre>
    </div>
  )
}

/** A unified diff with old and new line numbers where the hunk ranges allow (idea 26). */
function DiffBlock({ rows, original, language }: { rows: DiffRow[]; original: string; language: string }) {
  const [copied, setCopied] = useState(false)
  const copy = () => {
    void window.codync.app.copy(original)
    setCopied(true)
    window.setTimeout(() => setCopied(false), 1500)
  }
  return (
    <div className="md-code">
      <div className="md-code-header">
        <Text size="caption" mono tone="secondary" className="md-code-label">{language.trim() || 'diff'}</Text>
        <button className="md-code-action" aria-label={copied ? 'Copied' : 'Copy code'} title={copied ? 'Copied' : 'Copy code'} onClick={copy}>
          <Icon name={copied ? 'checkmark' : 'doc.on.doc'} size={12} weight="medium" />
        </button>
      </div>
      <div className="md-code-text md-diff">
        {rows.map((row, i) => <DiffLine key={i} row={row} />)}
      </div>
    </div>
  )
}

function DiffLine({ row }: { row: DiffRow }) {
  const numbered = row.kind === 'context' || row.kind === 'added' || row.kind === 'removed'
  const tone = row.kind === 'added' ? 'added' : row.kind === 'removed' ? 'removed' : row.kind === 'header' || row.kind === 'hunk' || row.kind === 'note' ? 'secondary' : 'text'
  const tint = row.kind === 'added' ? 'added' : row.kind === 'removed' ? 'removed' : row.kind === 'hunk' ? 'accent' : null
  return (
    <div className={`md-diff-line ${tint ? `tint-${tint}` : ''}`}>
      {numbered ? (
        <>
          <span className="md-diff-num">{row.kind === 'removed' || row.kind === 'context' ? row.oldNumber ?? '' : ''}</span>
          <span className="md-diff-num">{row.kind === 'added' || row.kind === 'context' ? row.newNumber ?? '' : ''}</span>
        </>
      ) : null}
      <Text as="span" mono tone={tone} className="md-diff-text">{row.raw || ' '}</Text>
    </div>
  )
}

/** The code with local highlighting; unknown languages and huge blocks stay plain text. */
function highlighted(text: string, language: string): ReactNode {
  const spans = highlightSpans(text, language)
  if (!spans.length) return text
  const out: ReactNode[] = []
  let pos = 0
  spans.forEach((s, i) => {
    if (s.start > pos) out.push(text.slice(pos, s.start))
    out.push(<span key={i} className={`hl-${s.kind}`}>{text.slice(s.start, s.end)}</span>)
    pos = s.end
  })
  if (pos < text.length) out.push(text.slice(pos))
  return out
}

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
