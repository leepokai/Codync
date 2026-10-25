import { useRef, useState, type ReactNode, type RefObject } from 'react'
import { createPortal } from 'react-dom'
import { isGroup, type Bot } from '@shared/models'
import { moveInOrder, rosterBand } from '../lib/computer-roster'
import { pointerDrag, type Point } from '../lib/pointer-drag'
import { reduceMotion } from '../lib/theme'
import { useSlide } from '../lib/slide'
import { sameRef, refKey, type RosterItem } from '../store/app-model'
import type { BotStore } from '../store/bot-store'
import type { useComputerRoster } from '../store/computer-roster'
import { StoreContext, useApp } from '../store/context'
import { BotRow } from './BotRow'
import { ComputerHeadingLabel, ComputerRosterSection } from './ComputerRosterSection'
import { UpdateNeededCard } from './UpdateNeededCard'

/** A bot being dragged among its computer's bots, and the order it has made so far. */
interface BotDrag {
  item: RosterItem
  ids: string[]
}

/** A computer heading being dragged, and the section order it has made so far. */
interface SectionDrag {
  id: string
  ids: string[]
}

/** The lifted copy of a dragged row or computer heading: its width, and where in it the pointer holds it. */
interface Lift {
  held: { item: RosterItem } | { store: BotStore }
  width: number
  grab: Point
  at: Point
  seq: number
}

export function BotRoster({ roster, compact, onContext, openExtensions }: {
  roster: ReturnType<typeof useComputerRoster>
  compact: boolean
  onContext: (bot: Bot, store: BotStore, x: number, y: number) => void
  /** A computer's own Extensions, from its heading. */
  openExtensions: (computerId: string) => void
}) {
  const app = useApp()
  const selected = app.selection
  const [hovered, setHovered] = useState<string | null>(null)
  const [botDrag, setBotDragState] = useState<BotDrag | null>(null)
  // The drag's handlers outlive the render that started it, so they read the latest drag here.
  const drag = useRef<BotDrag | null>(null)
  const setBotDrag = (next: BotDrag | null) => {
    drag.current = next
    setBotDragState(next)
  }
  // While a heading is dragged every section folds to it (as on the iPhone) and the order follows the pointer.
  const [sectionDrag, setSectionDragState] = useState<SectionDrag | null>(null)
  const headingDrag = useRef<SectionDrag | null>(null)
  const setSectionDrag = (next: SectionDrag | null) => {
    headingDrag.current = next
    setSectionDragState(next)
  }
  const [lift, setLift] = useState<Lift | null>(null)
  const ghost = useRef<HTMLDivElement>(null)
  const grab = useRef<Point>({ x: 0, y: 0 })
  const settle = useRef<ReturnType<typeof setTimeout> | null>(null)
  // Near the roster's top or bottom edge a drag scrolls it, from the last point the pointer was at.
  const pointer = useRef<Point>({ x: 0, y: 0 })
  const edgeScroll = useRef(0)
  const itemsByKey = useRef(new Map<string, RosterItem>())
  itemsByKey.current = new Map(roster.sections.flatMap((s) => s.items).map((item) => [refKey(item.ref), item]))

  /** A section's rows, in the order a drag in progress has them. */
  const ordered = (computerId: string, items: RosterItem[]) => {
    if (botDrag?.item.ref.computerId !== computerId) return items
    const byId = new Map(items.map((item) => [item.bot.id, item]))
    return botDrag.ids.flatMap((id) => byId.get(id) ?? [])
  }

  // The held element lifts: a copy follows the pointer from where it was grabbed; its slot stays open.
  const startLift = (held: Lift['held'], element: HTMLElement | null, from: Point) => {
    // A lift during the last one's settle takes over from it.
    if (settle.current) clearTimeout(settle.current)
    settle.current = null
    const rect = element?.getBoundingClientRect()
    if (!rect) return
    grab.current = { x: from.x - rect.left, y: from.y - rect.top }
    setLift((last) => ({ held, width: rect.width, grab: grab.current, at: from, seq: (last?.seq ?? 0) + 1 }))
  }

  const liftRow = (item: RosterItem, siblings: RosterItem[], from: Point) => {
    setBotDrag({ item, ids: siblings.map((s) => s.bot.id) })
    startLift({ item }, rowElement(item), from)
  }

  const liftHeading = (store: BotStore, from: Point) => {
    setSectionDrag({ id: store.computer.id, ids: roster.sections.map((s) => s.computer.id) })
    startLift({ store }, headingElement(store.computer.id), from)
  }

  // Passing over another section moves the held one to its place. Folded, the list is short, so
  // it doesn't scroll at the edges like a bot drag.
  const headingOver = (at: Point, under: Element | null) => {
    if (ghost.current) ghost.current.style.translate = ghostTranslate(at, grab.current)
    const current = headingDrag.current
    const section = under?.closest<HTMLElement>('[data-computer-section]')
    const target = section?.dataset.computerSection
    // A section still sliding out of the way would be hit again where it was.
    if (!current || !section || !target || target === current.id || section.getAnimations().length) return
    setSectionDrag({ ...current, ids: moveInOrder(current.ids, current.id, target) })
  }

  const dropHeading = () => {
    const current = headingDrag.current
    if (!current) return endDrag()
    const target = roster.sections[current.ids.indexOf(current.id)]?.computer.id
    if (target && target !== current.id) roster.move(current.id, target)
    const section = headingElement(current.id)?.closest<HTMLElement>('[data-computer-section]')
    for (const animation of section?.getAnimations() ?? []) animation.finish()
    settleInto(headingElement(current.id))
  }

  const dragOver = (at: Point, under: Element | null) => {
    pointer.current = at
    if (ghost.current) ghost.current.style.translate = ghostTranslate(at, grab.current)
    makeRoom(under)
    if (!edgeScroll.current) scrollAtEdge()
  }

  const scrollAtEdge = () => {
    const current = drag.current
    const scroller = current && rowElement(current.item)?.closest<HTMLElement>('.roster')
    if (!scroller) return void (edgeScroll.current = 0)
    const box = scroller.getBoundingClientRect()
    const { x, y } = pointer.current
    const reach = Math.max(0, box.top + EDGE - y) - Math.max(0, y - (box.bottom - EDGE))
    if (!reach) return void (edgeScroll.current = 0)
    scroller.scrollTop -= Math.round((reach / EDGE) * EDGE_SPEED)
    makeRoom(document.elementFromPoint(x, y))
    edgeScroll.current = requestAnimationFrame(scrollAtEdge)
  }

  // Any row of the same computer is a target; only rows in the same band (lead, pinned, the rest) make room.
  const makeRoom = (under: Element | null) => {
    const current = drag.current
    const row = under?.closest<HTMLElement>('[data-roster-row]')
    const item = row && itemsByKey.current.get(row.dataset.rosterRow!)
    if (!current || !row || !item || current.item.ref.computerId !== item.ref.computerId) return
    // A row still sliding out of the way would be hit again where it was.
    if (item.bot.id === current.item.bot.id || rosterBand(item.bot) !== rosterBand(current.item.bot) || row.getAnimations().length) return
    setBotDrag({ ...current, ids: moveInOrder(current.ids, current.item.bot.id, item.bot.id) })
  }

  const endDrag = () => {
    cancelAnimationFrame(edgeScroll.current)
    edgeScroll.current = 0
    settle.current = null
    setBotDrag(null)
    setSectionDrag(null)
    setLift(null)
  }

  const drop = () => {
    const current = drag.current
    if (!current) return endDrag()
    cancelAnimationFrame(edgeScroll.current)
    if (current.ids.some((id, index) => id !== current.item.store.roster[index]?.id)) current.item.store.reorder(current.ids)
    const row = rowElement(current.item)
    for (const animation of row?.getAnimations() ?? []) animation.finish()
    settleInto(row)
  }

  // The copy settles into the held element's new slot (when it's in view), then the element shows
  // again. Measured where it ends up, so callers finish its slide first.
  const settleInto = (element: HTMLElement | null) => {
    const slot = element?.getBoundingClientRect()
    const view = element?.closest('.roster')?.getBoundingClientRect()
    const visible = slot && view && slot.bottom > view.top && slot.top < view.bottom
    if (!slot || !visible || !ghost.current || reduceMotion()) return endDrag()
    ghost.current.classList.add('settling')
    ghost.current.style.translate = `${slot.left}px ${slot.top}px`
    settle.current = setTimeout(endDrag, SETTLE_MS)
  }

  const row = (item: RosterItem, siblings: RosterItem[]) => {
    const key = refKey(item.ref)
    const isSelected = sameRef(item.ref, selected)
    // The held row shows as hovered, or as an empty slot while its lifted copy follows the pointer.
    const held = botDrag?.item.bot.id === item.bot.id && botDrag.item.ref.computerId === item.ref.computerId
    const many = roster.sections.length > 1
    return (
      <RosterRow
        key={key}
        hovered={hovered === key || held}
        compact={compact}
        label={many ? `${item.bot.name}, on ${item.store.hostName}` : item.bot.name}
        title={many ? `${item.bot.name} · ${item.store.hostName}` : item.bot.name}
        selected={isSelected}
        dragKey={key}
        onHover={(on) => setHovered((h) => (on ? key : h === key ? null : h))}
        onClick={() => app.select(item.ref)}
        onContext={(x, y) => onContext(item.bot, item.store, x, y)}
        lifted={held && lift !== null}
        // The lead stays on top.
        drag={item.bot.lead ? undefined : pointerDrag({ start: (from) => liftRow(item, siblings, from), over: dragOver, drop, cancel: endDrag })}
      >
        <RosterBotRow item={item} compact={compact} />
      </RosterRow>
    )
  }

  const sectionOrder = sectionDrag ? sectionDrag.ids.flatMap((id) => roster.sections.find((s) => s.computer.id === id) ?? []) : roster.sections
  const sections = sectionOrder.map(({ computer, store, items }) => {
    const index = roster.sections.findIndex((s) => s.computer.id === computer.id)
    const notice = store.mismatch && !compact ? <StoreContext.Provider value={store}><UpdateNeededCard /></StoreContext.Provider> : null
    const rows = ordered(computer.id, items).map((item) => row(item, items))
    if (!roster.grouped) return <div key={computer.id}>{notice}{rows}</div>
    return (
      <ComputerRosterSection key={computer.id} store={store} compact={compact}
        openExtensions={store.shownConnection.kind === 'online' && !store.mismatch ? () => openExtensions(computer.id) : undefined}
        collapsed={sectionDrag !== null || roster.collapsed.includes(computer.id)} toggle={() => roster.toggle(computer.id)}
        previous={roster.sections[index - 1]?.computer.id} next={roster.sections[index + 1]?.computer.id}
        lifted={sectionDrag?.id === computer.id && lift !== null}
        drag={pointerDrag({ start: (from) => liftHeading(store, from), over: headingOver, drop: dropHeading, cancel: endDrag })}
        move={roster.move}>
        {/* The notice folds with its computer's bots; the heading still reads "Needs update". */}
        {notice}
        {rows}
        {!items.length && !store.mismatch && !compact ? <div className="computer-roster-empty">{store.shownConnection.kind === 'online' ? 'No bots yet' : store.connectionLabel}</div> : null}
      </ComputerRosterSection>
    )
  })
  return (
    <>
      {sections}
      {lift ? <RosterGhost key={lift.seq} lift={lift} ghost={ghost} compact={compact} /> : null}
    </>
  )
}

/** The lifted row or heading under the pointer (iOS's lifted cell): a smaller, translucent copy that follows it. */
function RosterGhost({ lift, ghost, compact }: { lift: Lift; ghost: RefObject<HTMLDivElement | null>; compact: boolean }) {
  const card = 'item' in lift.held ? `roster-row ${compact ? 'compact' : ''}` : 'computer-roster-toggle'
  return createPortal(
    <div ref={ghost} className="roster-ghost" style={{ width: lift.width, translate: ghostTranslate(lift.at, lift.grab) }}>
      <div className={`${card} roster-ghost-card`} style={{ transformOrigin: `${lift.grab.x}px ${lift.grab.y}px` }}>
        {'item' in lift.held ? <RosterBotRow item={lift.held.item} compact={compact} /> : <ComputerHeadingLabel store={lift.held.store} compact={compact} collapsed />}
      </div>
    </div>,
    document.body,
  )
}

function RosterBotRow({ item, compact }: { item: RosterItem; compact: boolean }) {
  return <BotRow bot={item.bot} store={item.store} compact={compact} usingComputer={item.store.screen?.agentBot === item.bot.id} members={isGroup(item.bot) ? item.store.members(item.bot) : noMembers} />
}

/** One roster button: drags to reorder, and slides when the others move. */
function RosterRow({ hovered, lifted, compact, label, title, selected, dragKey, onHover, onClick, onContext, drag, children }: {
  hovered: boolean
  /** Its copy is under the pointer: the row keeps its slot empty. */
  lifted: boolean
  compact: boolean
  label: string
  title: string
  selected: boolean
  dragKey: string
  onHover: (on: boolean) => void
  onClick: () => void
  onContext: (x: number, y: number) => void
  drag?: ReturnType<typeof pointerDrag>
  children: ReactNode
}) {
  const ref = useRef<HTMLButtonElement>(null)
  useSlide(ref)
  return (
    <button
      ref={ref}
      className={`roster-row ${compact ? 'compact' : ''} ${hovered ? 'hovered' : ''} ${selected ? 'selected' : ''} ${lifted ? 'lifted' : ''}`}
      aria-label={label}
      aria-current={selected || undefined}
      title={title}
      data-roster-row={dragKey}
      onMouseEnter={() => onHover(true)}
      onMouseLeave={() => onHover(false)}
      onClick={onClick}
      onContextMenu={(e) => {
        e.preventDefault()
        onContext(e.clientX, e.clientY)
      }}
      {...drag}
    >
      {children}
    </button>
  )
}

const noMembers: Bot[] = []

/** How long the lifted copy takes to settle into its slot (`--morph`). */
const SETTLE_MS = 200

/** How close to the roster's edge a drag starts scrolling it, and how fast (points per frame). */
const EDGE = 36
const EDGE_SPEED = 12

function rowElement(item: RosterItem): HTMLElement | null {
  return document.querySelector<HTMLElement>(`[data-roster-row="${CSS.escape(refKey(item.ref))}"]`)
}

function headingElement(computerId: string): HTMLElement | null {
  return document.querySelector<HTMLElement>(`[data-computer-heading="${CSS.escape(computerId)}"]`)
}

/** The lifted copy's `translate` that keeps the point it was grabbed by under the pointer. */
function ghostTranslate(at: Point, grab: Point): string {
  return `${at.x - grab.x}px ${at.y - grab.y}px`
}
