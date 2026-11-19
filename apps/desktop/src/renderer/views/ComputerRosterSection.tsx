import { useRef, type ReactNode } from 'react'
import { IconButton } from '../components/Controls'
import { Icon } from '../components/Icon'
import type { pointerDrag } from '../lib/pointer-drag'
import { useTr } from '../lib/use-language'
import { fill } from '@shared/fill'
import { useSlide } from '../lib/slide'
import type { BotStore } from '../store/bot-store'
import { ComputerBadge } from './ComputerBadge'
import './computer-roster.css'

interface Props {
  store: BotStore
  collapsed: boolean
  previous?: string
  next?: string
  /** Its heading's copy is under the pointer: the section keeps its slot empty. */
  lifted: boolean
  drag: ReturnType<typeof pointerDrag>
  toggle: () => void
  move: (id: string, target: string) => void
  /** This computer's own Extensions; absent while it can't be reached. */
  openExtensions?: () => void
  /** Its screen, for another computer that has one; absent otherwise. */
  openScreen?: () => void
  /** This computer's workflows, beside Extensions. */
  openWorkflows?: () => void
  children: ReactNode
}

export function ComputerRosterSection({ store, collapsed, previous, next, lifted, drag, toggle, move, openExtensions, openScreen, openWorkflows, children }: Props) {
  const tr = useTr()
  const section = useRef<HTMLElement>(null)
  const id = store.computer.id
  useSlide(section)
  return (
    <section ref={section} className={`computer-roster-section ${lifted ? 'lifted' : ''}`} aria-label={store.hostName} data-computer-section={id}>
      <div className="computer-roster-heading">
        <button className="computer-roster-toggle" aria-expanded={!collapsed} data-computer-heading={id}
          aria-label={`${store.hostName}, ${store.connectionLabel}`} title={fill(tr('{name} · {other}. Drag to reorder.'), { name: store.hostName, other: store.connectionLabel })}
          onClick={toggle}
          {...drag}
          onKeyDown={(event) => {
            if (!event.altKey || !['ArrowUp', 'ArrowDown'].includes(event.key)) return
            event.preventDefault()
            event.stopPropagation()
            const target = event.key === 'ArrowUp' ? previous : next
            if (target) move(id, target)
          }}>
          <ComputerHeadingLabel store={store} collapsed={collapsed} />
        </button>
        {openScreen ? <IconButton className="computer-roster-action" title="Screen" icon="display" size={24} onClick={openScreen} /> : null}
        {openWorkflows ? <IconButton className="computer-roster-action" title="Workflows" icon="flowchart" size={24} onClick={openWorkflows} /> : null}
        {openExtensions ? <IconButton className="computer-roster-action" title="Extensions" icon="square.grid.2x2" size={24} onClick={openExtensions} /> : null}
      </div>
      <div className={`computer-roster-content ${collapsed ? 'collapsed' : ''}`} inert={collapsed} aria-hidden={collapsed}>
        <div>{children}</div>
      </div>
    </section>
  )
}

/** A heading's badge, name, connection and fold chevron; also its lifted copy while dragged. */
export function ComputerHeadingLabel({ store, collapsed }: { store: BotStore; collapsed: boolean }) {
  const tr = useTr()
  return (
    <>
      <ComputerBadge computer={store.computer} size={20} />
      <span className="computer-roster-label">
        <strong>{store.hostName}</strong>
        <span className={`computer-roster-status ${store.isOffline && !store.isUpdating && !store.isAsleep ? 'warning' : ''}`}>{tr(store.connectionLabel)}</span>
      </span>
      <span className={`computer-roster-chevron ${collapsed ? 'collapsed' : ''}`}><Icon name="chevron.down" size={9} /></span>
    </>
  )
}
