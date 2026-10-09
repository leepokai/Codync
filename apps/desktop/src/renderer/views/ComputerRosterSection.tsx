import { useLayoutEffect, useRef, type ReactNode } from 'react'
import { Icon } from '../components/Icon'
import { reduceMotion } from '../lib/theme'
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
  const lastTop = useRef<number | null>(null)
  const id = store.computer.id
  useLayoutEffect(() => {
    const element = section.current
    if (!element) return
    const top = element.offsetTop
    if (lastTop.current !== null && top !== lastTop.current && !reduceMotion()) {
      element.animate([{ transform: `translateY(${lastTop.current - top}px)` }, { transform: 'translateY(0)' }], { duration: 300, easing: 'ease-out' })
    }
    lastTop.current = top
  })
  return (
    <section ref={section} className={`computer-roster-section ${dragged === id ? 'dragging' : ''}`} aria-label={store.hostName}
      onDragOver={(event) => {
        if (!dragged || dragged === id) return
        event.preventDefault()
        event.dataTransfer.dropEffect = 'move'
      }}
      onDrop={(event) => {
        if (!dragged) return
        event.preventDefault()
        move(dragged, id)
        setDragged(null)
      }}>
      <div className="computer-roster-heading">
        <button className="computer-roster-toggle" draggable aria-expanded={!collapsed}
          aria-label={`${store.hostName}, ${store.connectionLabel}`} title={`${store.hostName} · ${store.connectionLabel}. Drag to reorder.`}
          onClick={toggle}
          onDragStart={(event) => {
            event.dataTransfer.effectAllowed = 'move'
            event.dataTransfer.setData('text/plain', id)
            setDragged(id)
          }}
          onDragEnd={() => setDragged(null)}
          onKeyDown={(event) => {
            if (!event.altKey || !['ArrowUp', 'ArrowDown'].includes(event.key)) return
            event.preventDefault()
            event.stopPropagation()
            const target = event.key === 'ArrowUp' ? previous : next
            if (target) move(id, target)
          }}>
          <ComputerBadge computer={store.computer} size={20} />
          {!compact ? <span className="computer-roster-label"><strong>{store.hostName}</strong><span className={store.shownConnection.kind === 'online' ? '' : 'warning'}>{store.connectionLabel}</span></span> : null}
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
