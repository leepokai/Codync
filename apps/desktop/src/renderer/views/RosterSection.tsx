import { useRef, type ReactNode } from 'react'
import { Icon } from '../components/Icon'
import { HStack } from '../components/Stack'
import { Text } from '../components/Text'
import { useSlide } from '../lib/slide'
import type { pointerDrag } from '../lib/pointer-drag'
import { useTr } from '../lib/use-language'
import './computer-roster.css'

interface Props {
  name: string
  count: number
  /** Its bots' unread messages, shown while it's folded. */
  unread: number
  folded: boolean
  compact: boolean
  toggle: () => void
  onContext: (x: number, y: number) => void
  drag?: ReturnType<typeof pointerDrag>
  move?: (offset: number) => void
  children: ReactNode
}

/** A section the user made (Slack's custom sidebar sections): its header folds its rows. */
export function RosterSection({ name, count, unread, folded, compact, toggle, onContext, drag, move, children }: Props) {
  const section = useRef<HTMLElement>(null)
  useSlide(section)
  return (
    <section ref={section} aria-label={name}>
      <button data-section-heading className="roster-section-toggle" {...drag} onKeyDown={(e) => {
        if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) { e.preventDefault(); e.stopPropagation(); move?.(e.key === 'ArrowUp' ? -1 : 1) }
      }} aria-expanded={!folded} aria-label={name} title={name} onClick={toggle}
        onContextMenu={(e) => {
          e.preventDefault()
          onContext(e.clientX, e.clientY)
        }}>
        <HStack gap={5} justify={compact ? 'center' : undefined}>
          <span className={`computer-roster-chevron ${folded ? 'collapsed' : ''}`}><Icon name="chevron.down" size={9} /></span>
          {!compact ? <Text size="caption" weight="semibold" truncate grow>{name}</Text> : null}
          {!compact ? <Text size="caption" tone="tertiary" tabular>{count}</Text> : null}
          {folded && unread > 0 ? <span className="unread-badge">{unread}</span> : null}
        </HStack>
      </button>
      <div className={`computer-roster-content ${folded ? 'collapsed' : ''}`} inert={folded} aria-hidden={folded}>
        <div>{children}</div>
      </div>
    </section>
  )
}

/** The plain "Bots" label over the bots in no section, set like a section's name; it folds nothing.
 *  The narrow rail drops its text, as headers drop their names, and keeps the gap. */
export function RosterLabel({ compact, drag, move }: { compact: boolean; drag?: ReturnType<typeof pointerDrag>; move?: (offset: number) => void }) {
  const tr = useTr()
  const label = useRef<HTMLButtonElement>(null)
  useSlide(label)
  return <button ref={label} data-section-heading className="roster-label" {...drag} aria-label={tr('Bots')}
    onKeyDown={(e) => {
      if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) { e.preventDefault(); e.stopPropagation(); move?.(e.key === 'ArrowUp' ? -1 : 1) }
    }}>
    {!compact ? <Text as="span" size="caption" weight="semibold" tone="secondary">{tr('Bots')}</Text> : null}
  </button>
}
