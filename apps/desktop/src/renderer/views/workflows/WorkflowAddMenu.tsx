import { useEffect, useRef, useState, type KeyboardEvent, type RefObject } from 'react'
import { SearchField } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { AnchoredMenu } from '../../components/Overlay'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import type { Branch, NodeKind, Point } from './workflow'

const GROUPS: { label: string; kinds: { kind: NodeKind; title: string; icon: string }[] }[] = [
  {
    label: 'Input',
    kinds: [
      { kind: 'trigger', title: 'Trigger', icon: 'bolt' },
      { kind: 'prompt', title: 'Prompt', icon: 'text.bubble' },
    ],
  },
  {
    label: 'Steps',
    kinds: [
      { kind: 'agent', title: 'Agent', icon: 'person.fill' },
    ],
  },
  {
    label: 'Flow',
    kinds: [
      { kind: 'condition', title: 'Condition', icon: 'arrow.triangle.branch' },
      { kind: 'approval', title: 'Approval', icon: 'person.crop.circle.badge.checkmark' },
      { kind: 'output', title: 'Output', icon: 'paperplane' },
    ],
  },
]

export function WorkflowAddMenu({ open, point, anchor, at, link, onAdd, onClose }: {
  open?: boolean
  point?: { x: number; y: number } | null
  anchor?: RefObject<HTMLElement | null>
  at?: Point | null
  link?: { from: string; branch?: Branch | null; at: Point } | null
  onAdd: (kind: NodeKind, at: Point, link?: { from: string; branch?: Branch | null }) => void
  onClose: () => void
}) {
  const tr = useTr()
  const shown = open ?? point != null
  const input = useRef<HTMLInputElement>(null)
  const [query, setQuery] = useState('')
  const [highlight, setHighlight] = useState(0)
  const needle = query.trim().toLowerCase()
  const groups = GROUPS.map((group) => ({
    ...group,
    kinds: group.kinds.filter((item) => !needle || tr(item.title).toLowerCase().includes(needle) || item.title.toLowerCase().includes(needle)),
  })).filter((group) => group.kinds.length > 0)
  const flat = groups.flatMap((group) => group.kinds)

  useEffect(() => {
    if (!shown) {
      setQuery('')
      setHighlight(0)
      return
    }
    const frame = requestAnimationFrame(() => input.current?.focus())
    return () => cancelAnimationFrame(frame)
  }, [shown])

  useEffect(() => { setHighlight(0) }, [needle])

  const pick = (kind: NodeKind) => onAdd(kind, link?.at ?? at ?? { x: 80, y: 80 }, link ?? undefined)

  const onKey = (event: KeyboardEvent) => {
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault()
      if (!flat.length) return
      setHighlight((index) => (index + (event.key === 'ArrowDown' ? 1 : flat.length - 1)) % flat.length)
    } else if (event.key === 'Enter' && flat[highlight]) {
      event.preventDefault()
      pick(flat[highlight].kind)
      onClose()
    }
  }

  return (
    <AnchoredMenu open={shown} point={point} anchor={anchor} onClose={onClose} className="workflow-add-panel">
      <div className="workflow-add-menu" onKeyDown={onKey}>
        <div className="workflow-add-search">
          <SearchField inputRef={input} value={query} prompt="Search nodes" onChange={setQuery} />
        </div>
        <div className="workflow-add-list" role="menu">
          {flat.length === 0 ? <Text size="caption" tone="secondary">{tr('No matches')}</Text> : null}
          {groups.map((group) => (
            <div key={group.label}>
              <div className="workflow-add-heading"><Text size="caption" tone="secondary">{tr(group.label)}</Text></div>
              {group.kinds.map((item) => {
                const index = flat.indexOf(item)
                return (
                  <button
                    key={item.kind}
                    type="button"
                    role="menuitem"
                    className={`menu-row ${index === highlight ? 'highlight' : ''}`}
                    onMouseEnter={() => setHighlight(index)}
                    onClick={() => { pick(item.kind); onClose() }}
                  >
                    <span className="workflow-add-icon"><Icon name={item.icon} size={14} /></span>
                    <span className="menu-title">{tr(item.title)}</span>
                  </button>
                )
              })}
            </div>
          ))}
        </div>
      </div>
    </AnchoredMenu>
  )
}
