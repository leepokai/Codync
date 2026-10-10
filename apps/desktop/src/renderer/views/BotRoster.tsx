import { useRef, useState, type DragEvent, type ReactNode } from 'react'
import { isGroup, type Bot } from '@shared/models'
import { moveInOrder } from '../lib/computer-roster'
import { useSlide } from '../lib/slide'
import { sameRef, refKey, type RosterItem } from '../store/app-model'
import type { BotStore } from '../store/bot-store'
import type { useComputerRoster } from '../store/computer-roster'
import { StoreContext, useApp } from '../store/context'
import { BotRow } from './BotRow'
import { ComputerRosterSection } from './ComputerRosterSection'
import { UpdateNeededCard } from './UpdateNeededCard'

/** A bot being dragged among its computer's bots, and the order it has made so far. */
interface BotDrag {
  item: RosterItem
  ids: string[]
}

export function BotRoster({ roster, compact, onContext }: {
  roster: ReturnType<typeof useComputerRoster>
  compact: boolean
  onContext: (bot: Bot, store: BotStore, x: number, y: number) => void
}) {
  const app = useApp()
  const selected = app.selection
  const [hovered, setHovered] = useState<string | null>(null)
  const [dragged, setDragged] = useState<string | null>(null)
  const [botDrag, setBotDrag] = useState<BotDrag | null>(null)

  /** A section's rows, in the order a drag in progress has them. */
  const ordered = (computerId: string, items: RosterItem[]) => {
    if (botDrag?.item.ref.computerId !== computerId) return items
    const byId = new Map(items.map((item) => [item.bot.id, item]))
    return botDrag.ids.flatMap((id) => byId.get(id) ?? [])
  }

  // Any row of the same computer takes the drop (on enter and over); only rows with the same pin state make room.
  const dragOver = (event: DragEvent<HTMLElement>, item: RosterItem) => {
    if (!botDrag || botDrag.item.ref.computerId !== item.ref.computerId) return
    event.preventDefault()
    event.dataTransfer.dropEffect = 'move'
    // A row still sliding out of the way would be hit again where it was.
    if (item.bot.id === botDrag.item.bot.id || item.bot.pinned !== botDrag.item.bot.pinned || event.currentTarget.getAnimations().length) return
    setBotDrag({ ...botDrag, ids: moveInOrder(botDrag.ids, botDrag.item.bot.id, item.bot.id) })
  }

  const drop = (event: DragEvent<HTMLElement>) => {
    if (!botDrag) return
    event.preventDefault()
    if (botDrag.ids.some((id, index) => id !== botDrag.item.store.roster[index]?.id)) botDrag.item.store.reorder(botDrag.ids)
    setBotDrag(null)
  }

  const row = (item: RosterItem, siblings: RosterItem[]) => {
    const key = refKey(item.ref)
    const isSelected = sameRef(item.ref, selected)
    const many = roster.sections.length > 1
    return (
      <RosterRow
        key={key}
        dragging={botDrag?.item.bot.id === item.bot.id && botDrag.item.ref.computerId === item.ref.computerId}
        background={isSelected ? 'var(--roster-selected)' : hovered === key ? 'var(--bubble-agent)' : 'transparent'}
        compact={compact}
        label={many ? `${item.bot.name}, on ${item.store.hostName}` : item.bot.name}
        title={many ? `${item.bot.name} · ${item.store.hostName}` : item.bot.name}
        selected={isSelected}
        onHover={(on) => setHovered((h) => (on ? key : h === key ? null : h))}
        onClick={() => app.select(item.ref)}
        onContext={(x, y) => onContext(item.bot, item.store, x, y)}
        onDragStart={() => setBotDrag({ item, ids: siblings.map((s) => s.bot.id) })}
        onDragOver={(event) => dragOver(event, item)}
        onDrop={drop}
        onDragEnd={() => setBotDrag(null)}
      >
        <BotRow bot={item.bot} store={item.store} compact={compact} usingComputer={item.store.screen?.agentBot === item.bot.id} members={isGroup(item.bot) ? item.store.members(item.bot) : noMembers} />
      </RosterRow>
    )
  }

  return roster.sections.map(({ computer, store, items }, index) => {
    const notice = store.mismatch && !compact ? <StoreContext.Provider value={store}><UpdateNeededCard /></StoreContext.Provider> : null
    const rows = ordered(computer.id, items).map((item) => row(item, items))
    if (!roster.grouped) return <div key={computer.id}>{notice}{rows}</div>
    return (
      <ComputerRosterSection key={computer.id} store={store} compact={compact}
        collapsed={roster.collapsed.includes(computer.id)} toggle={() => roster.toggle(computer.id)}
        previous={roster.sections[index - 1]?.computer.id} next={roster.sections[index + 1]?.computer.id}
        dragged={dragged} setDragged={setDragged} move={roster.move} notice={notice}>
        {rows}
        {!items.length && !store.mismatch && !compact ? <div className="computer-roster-empty">{store.shownConnection.kind === 'online' ? 'No bots yet' : store.connectionLabel}</div> : null}
      </ComputerRosterSection>
    )
  })
}

/** One roster button: drags to reorder, and slides when the others move. */
function RosterRow({ dragging, background, compact, label, title, selected, onHover, onClick, onContext, onDragStart, onDragOver, onDrop, onDragEnd, children }: {
  dragging: boolean
  background: string
  compact: boolean
  label: string
  title: string
  selected: boolean
  onHover: (on: boolean) => void
  onClick: () => void
  onContext: (x: number, y: number) => void
  onDragStart: () => void
  onDragOver: (event: DragEvent<HTMLElement>) => void
  onDrop: (event: DragEvent<HTMLElement>) => void
  onDragEnd: () => void
  children: ReactNode
}) {
  const ref = useRef<HTMLButtonElement>(null)
  useSlide(ref)
  return (
    <button
      ref={ref}
      className={`roster-row ${dragging ? 'dragging' : ''}`}
      style={{ padding: compact ? 0 : '0 8px', background }}
      aria-label={label}
      aria-selected={selected}
      title={title}
      draggable
      onMouseEnter={() => onHover(true)}
      onMouseLeave={() => onHover(false)}
      onClick={onClick}
      onContextMenu={(e) => {
        e.preventDefault()
        onContext(e.clientX, e.clientY)
      }}
      onDragStart={(e) => {
        // Its own type, so the composer and other apps don't take it as text.
        e.dataTransfer.effectAllowed = 'move'
        e.dataTransfer.setData('application/x-codync-bot', label)
        onDragStart()
      }}
      onDragEnter={onDragOver}
      onDragOver={onDragOver}
      onDrop={onDrop}
      onDragEnd={onDragEnd}
    >
      {children}
    </button>
  )
}

const noMembers: Bot[] = []
