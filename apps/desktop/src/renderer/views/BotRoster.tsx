import { useState } from 'react'
import { isGroup, type Bot } from '@shared/models'
import { sameRef, refKey, type RosterItem } from '../store/app-model'
import type { BotStore } from '../store/bot-store'
import type { useComputerRoster } from '../store/computer-roster'
import { StoreContext, useApp } from '../store/context'
import { BotRow } from './BotRow'
import { ComputerRosterSection } from './ComputerRosterSection'
import { UpdateNeededCard } from './UpdateNeededCard'

export function BotRoster({ roster, compact, onContext }: {
  roster: ReturnType<typeof useComputerRoster>
  compact: boolean
  onContext: (bot: Bot, store: BotStore, x: number, y: number) => void
}) {
  const app = useApp()
  const selected = app.selection
  const [hovered, setHovered] = useState<string | null>(null)
  const [dragged, setDragged] = useState<string | null>(null)
  const row = (item: RosterItem) => {
    const key = refKey(item.ref)
    const isSelected = sameRef(item.ref, selected)
    return (
      <button
        key={key}
        className="roster-row"
        style={{
          padding: compact ? 0 : '0 8px',
          background: isSelected ? 'var(--roster-selected)' : hovered === key ? 'var(--bubble-agent)' : 'transparent',
        }}
        aria-label={roster.sections.length > 1 ? `${item.bot.name}, on ${item.store.hostName}` : item.bot.name}
        aria-selected={isSelected}
        title={roster.sections.length > 1 ? `${item.bot.name} · ${item.store.hostName}` : item.bot.name}
        onMouseEnter={() => setHovered(key)}
        onMouseLeave={() => setHovered((h) => (h === key ? null : h))}
        onClick={() => app.select(item.ref)}
        onContextMenu={(e) => {
          e.preventDefault()
          onContext(item.bot, item.store, e.clientX, e.clientY)
        }}
      >
        <BotRow bot={item.bot} store={item.store} compact={compact} usingComputer={item.store.screen?.agentBot === item.bot.id} members={isGroup(item.bot) ? item.store.members(item.bot) : noMembers} />
      </button>
    )
  }

  return roster.sections.map(({ computer, store, items }, index) => {
    const notice = store.mismatch && !compact ? <StoreContext.Provider value={store}><UpdateNeededCard /></StoreContext.Provider> : null
    if (!roster.grouped) return <div key={computer.id}>{notice}{items.map(row)}</div>
    return (
      <ComputerRosterSection key={computer.id} store={store} compact={compact}
        collapsed={roster.collapsed.includes(computer.id)} toggle={() => roster.toggle(computer.id)}
        previous={roster.sections[index - 1]?.computer.id} next={roster.sections[index + 1]?.computer.id}
        dragged={dragged} setDragged={setDragged} move={roster.move} notice={notice}>
        {items.map(row)}
        {!items.length && !store.mismatch && !compact ? <div className="computer-roster-empty">{store.shownConnection.kind === 'online' ? 'No bots yet' : store.connectionLabel}</div> : null}
      </ComputerRosterSection>
    )
  })
}

const noMembers: Bot[] = []
