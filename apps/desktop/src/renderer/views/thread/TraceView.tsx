import { useLayoutEffect, useRef, useState } from 'react'
import type { Entry, EntryData } from '@shared/models'
import { CardSection, Disclosure } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader } from '../../components/Overlay'
import { ThinkingOrb, type OrbState } from '../../components/ThinkingOrb'
import { font } from '../../lib/fonts'
import { useStore } from '../../store/context'
import { MarkdownText } from './Markdown'
import { CodeBox, DiffView } from './PermissionCard'

/**
 * "Full conversation": everything the agent did (narration, thinking, tool calls with output
 * and diffs, plans) grouped by turn. A chat's trace, or one thread's (`thread`: its root).
 */
export function TraceView({ botId, thread = null }: { botId: string; thread?: string | null }) {
  const store = useStore()
  const turns = new Map<number, Entry[]>()
  for (const e of store.allEntries(botId)) {
    if ((e.threadId ?? null) !== thread) continue
    turns.set(e.turn, [...(turns.get(e.turn) ?? []), e])
  }
  const sorted = [...turns.entries()].sort((a, b) => a[0] - b[0])
  const scroller = useRef<HTMLDivElement>(null)
  useLayoutEffect(() => {
    const el = scroller.current
    if (el) el.scrollTop = el.scrollHeight
  }, [])
  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', background: 'var(--background)' }}>
      <ModalHeader title="Full conversation" />
      <div ref={scroller} style={{ flex: 1, overflowY: 'auto' }}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 14, padding: 14, fontSize: 'calc(12px * var(--scale))' }}>
          {sorted.length === 0 ? <span style={{ color: 'var(--tertiary)' }}>Nothing yet.</span> : null}
          {sorted.map(([turn, entries]) => (
            <div key={turn} style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
              <div style={{ ...font('compactSecondary', 'semibold'), color: 'var(--text)', paddingLeft: 4, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
                {entries.find((e) => e.kind === 'user')?.data.text ?? `Turn ${turn}`}
              </div>
              <CardSection>
                {entries.map((e) => (
                  <TraceRow key={e.id} entry={e} />
                ))}
              </CardSection>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}

function Label({ icon, children, style }: { icon: React.ReactNode; children: React.ReactNode; style?: React.CSSProperties }) {
  return (
    <span style={{ display: 'flex', alignItems: 'flex-start', gap: 6, ...style }}>
      <span style={{ display: 'flex', alignItems: 'center', minHeight: '1.25em' }}>{icon}</span>
      <span style={{ minWidth: 0 }}>{children}</span>
    </span>
  )
}

function TraceRow({ entry }: { entry: Entry }) {
  const [expanded, setExpanded] = useState(false)
  const d = entry.data
  switch (entry.kind) {
    case 'user':
      return (
        <Label icon={<Icon name="person.fill" size={11} color="var(--secondary)" />} style={font('subheadline')}>
          <span style={{ color: 'var(--text)' }}>{d.text ?? ''}</span>
        </Label>
      )
    case 'agent':
      return (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
          <span style={{ ...font('caption2', 'semibold'), color: 'var(--tertiary)' }}>{d.final ? 'Reply' : 'Said'}</span>
          <MarkdownText text={d.text ?? ''} />
        </div>
      )
    case 'thought':
      return (
        <Disclosure
          expanded={expanded}
          onToggle={() => setExpanded((x) => !x)}
          label={
            <span style={{ display: 'flex', alignItems: 'center', gap: 6, ...font('subheadline'), color: 'var(--secondary)' }}>
              <ThinkingOrb size={16} color="var(--secondary)" animated={false} />
              Thinking
            </span>
          }
        >
          <div className="selectable" style={{ ...font('footnote'), color: 'var(--secondary)', whiteSpace: 'pre-wrap' }}>{d.text ?? ''}</div>
        </Disclosure>
      )
    case 'tool':
      return <ToolRow data={d} />
    case 'plan':
      return (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
          <Label icon={<Icon name="checklist" size={11} weight="bold" />} style={font('subheadline', 'bold')}>
            Plan
          </Label>
          {(d.entries ?? []).map((item, i) => (
            <Label
              key={i}
              style={font('footnote')}
              icon={
                <Icon
                  name={item.status === 'completed' ? 'checkmark.circle.fill' : item.status === 'in_progress' ? 'circle.dotted' : 'circle'}
                  size={10}
                  color={item.status === 'completed' ? 'var(--accent)' : 'var(--tertiary)'}
                />
              }
            >
              <span style={{ color: 'var(--text)', textDecoration: item.status === 'completed' ? 'line-through' : undefined }}>{item.content}</span>
            </Label>
          ))}
        </div>
      )
    case 'permission':
      return (
        <Label icon={<Icon name="hand.raised" size={11} />} style={{ ...font('subheadline'), color: 'var(--secondary)' }}>
          {d.title ?? 'Approval'}
        </Label>
      )
    default:
      return <span style={{ ...font('footnote'), color: 'var(--tertiary)' }}>{d.text ?? ''}</span>
  }
}

const toolIcon = (kind?: string) =>
  ({ read: 'doc.text', edit: 'pencil', delete: 'trash', move: 'arrow.right.doc.on.clipboard', search: 'magnifyingglass', execute: 'terminal', think: 'brain', fetch: 'globe' })[kind ?? ''] ?? 'wrench.and.screwdriver'

const toolOrb = (kind?: string): OrbState => ({ search: 'searching', fetch: 'connecting', edit: 'shaping' } as const)[kind ?? ''] ?? 'working'

export function ToolRow({ data }: { data: EntryData }) {
  const [expanded, setExpanded] = useState(false)
  const hasBody = !!data.output || !!data.diffs?.length
  const label = (
    <span style={{ display: 'flex', alignItems: 'center', gap: 8, ...font('caption'), flex: 1, minWidth: 0 }}>
      <span style={{ width: 18, display: 'flex', justifyContent: 'center', color: 'var(--secondary)' }}>
        <Icon name={toolIcon(data.toolKind)} size={10} />
      </span>
      <span style={{ ...font('subheadline'), color: 'var(--text)', flex: 1, minWidth: 0, display: '-webkit-box', WebkitLineClamp: 2, WebkitBoxOrient: 'vertical', overflow: 'hidden' }}>{data.title ?? 'Tool'}</span>
      {data.status === 'completed' ? (
        <Icon name="checkmark" size={10} color="var(--accent)" />
      ) : data.status === 'failed' ? (
        <Icon name="xmark" size={10} color="var(--danger)" />
      ) : (
        <ThinkingOrb state={toolOrb(data.toolKind)} size={16} color="var(--secondary)" />
      )}
    </span>
  )
  if (!hasBody) return label
  return (
    <Disclosure expanded={expanded} onToggle={() => setExpanded((x) => !x)} label={label}>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
        {(data.diffs ?? []).map((diff, i) => <DiffView key={i} diff={diff} />)}
        {data.output ? <CodeBox text={data.output} /> : null}
      </div>
    </Disclosure>
  )
}
