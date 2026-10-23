import { useRef, type ReactNode } from 'react'
import { Icon } from '../components/Icon'
import { pointerDrag } from '../lib/pointer-drag'
import { useTr } from '../lib/use-language'
import { useSlide } from '../lib/slide'
import type { BotStore } from '../store/bot-store'
import { ComputerBadge } from './ComputerBadge'
import './computer-roster.css'

interface Props {
  store: BotStore
  compact: boolean
  collapsed: boolean
  previous?: string
  next?: string
  dragged: string | null
  setDragged: (id: string | null) => void
  toggle: () => void
  move: (id: string, target: string) => void
  notice: ReactNode
  children: ReactNode
}

export function ComputerRosterSection({ store, compact, collapsed, previous, next, dragged, setDragged, toggle, move, notice, children }: Props) {
  const section = useRef<HTMLElement>(null)
  const tr = useTr()
  const dropTarget = useRef<string | null>(null)
  const id = store.computer.id
  useSlide(section)
  const drag = pointerDrag({
    start: () => {
      dropTarget.current = null
      setDragged(id)
    },
    over: (_, under) => {
      dropTarget.current = under?.closest<HTMLElement>('[data-computer-section]')?.dataset.computerSection ?? null
    },
    drop: () => {
      if (dropTarget.current && dropTarget.current !== id) move(id, dropTarget.current)
      setDragged(null)
    },
    cancel: () => setDragged(null),
  })
  return (
    <section ref={section} className={`computer-roster-section ${dragged === id ? 'dragging' : ''}`} aria-label={store.hostName} data-computer-section={id}>
      <div className="computer-roster-heading">
        <button className="computer-roster-toggle" aria-expanded={!collapsed}
          aria-label={`${store.hostName}, ${store.connectionLabel}`} title={`${store.hostName} · ${store.connectionLabel}. Drag to reorder.`}
          onClick={toggle}
          {...drag}
          onKeyDown={(event) => {
            if (!event.altKey || !['ArrowUp', 'ArrowDown'].includes(event.key)) return
            event.preventDefault()
            event.stopPropagation()
            const target = event.key === 'ArrowUp' ? previous : next
            if (target) move(id, target)
          }}>
          <ComputerBadge computer={store.computer} size={20} />
          {!compact ? <span className="computer-roster-label"><strong>{store.hostName}</strong><span className={store.shownConnection.kind === 'online' ? '' : 'warning'}>{tr(store.connectionLabel)}</span></span> : null}
          <span className={`computer-roster-chevron ${collapsed ? 'collapsed' : ''}`}><Icon name="chevron.down" size={9} /></span>
        </button>
      </div>
      {notice}
      <div className={`computer-roster-content ${collapsed ? 'collapsed' : ''}`} inert={collapsed} aria-hidden={collapsed}>
        <div>{children}</div>
      </div>
    </section>
  )
}
