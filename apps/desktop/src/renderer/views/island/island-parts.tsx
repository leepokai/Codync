import { useRef } from 'react'
import type { IslandBot } from '@shared/island'
import { CharacterAvatar, type Mood } from '../../components/Avatar'
import { Icon } from '../../components/Icon'
import { ThinkingOrb } from '../../components/ThinkingOrb'
import { useArrivals, useFlip } from './island-motion'
import { avatarColor, palette, withAlpha } from '../../lib/theme'

// Pieces the island's views share: the big character with its glow and
// badge, the agent row, the column of other bots and the key hint on a button.

export type Badge = 'working' | 'needsInput' | 'finished' | 'failed' | null

/** State colors: amber needs you, green done, red failed (Codync's palette has no others). */
export const AMBER = palette(true).warning
export const GREEN = '#34D399'
export const RED = '#F4505E'

export const moodOf = (bot: Pick<IslandBot, 'status'>): Mood =>
  bot.status === 'needsInput' ? 'needsInput' : bot.status === 'working' ? 'working' : 'idle'

export const badgeOf = (bot: Pick<IslandBot, 'status'>): Badge =>
  bot.status === 'needsInput' ? 'needsInput' : bot.status === 'working' ? 'working' : bot.status === 'error' ? 'failed' : null

function glowOf(badge: Badge, color: string) {
  switch (badge) {
    case 'needsInput':
      return AMBER
    case 'finished':
      return GREEN
    case 'failed':
      return RED
    case 'working':
      return avatarColor(color)
    case null:
      return null
  }
}

/** The bot in the big slot: its character, a glow in its state's color and a badge. */
export function BigCharacter({ bot, size, mood, badge, onOpen }: {
  bot: Pick<IslandBot, 'shape' | 'color' | 'customIcon'>
  size: number
  mood: Mood
  badge: Badge
  onOpen?: () => void
}) {
  const glow = glowOf(badge, bot.color)
  const inner = (
    <>
      {glow ? <span aria-hidden className="island-glow" style={{ background: `radial-gradient(circle, ${withAlpha(glow, 0.36)}, ${withAlpha(glow, 0)} 68%)` }} /> : null}
      <CharacterAvatar customIcon={bot.customIcon} shape={bot.shape} color={bot.color} size={size} mood={mood} dark />
      <StateBadge badge={badge} />
    </>
  )
  const style = { width: size, height: size }
  return onOpen ? (
    <button className="island-big" style={style} onClick={onOpen} title="Open conversation" aria-label="Open conversation">
      {inner}
    </button>
  ) : (
    <span className="island-big" style={style}>
      {inner}
    </span>
  )
}

export function StateBadge({ badge, small = false }: { badge: Badge; small?: boolean }) {
  if (!badge) return null
  if (badge === 'working' && !small) {
    return (
      <span className="island-badge orb" aria-label="Working">
        <ThinkingOrb size={14} color="#F5F6F8" />
      </span>
    )
  }
  if (badge === 'working') {
    return (
      <span className={`island-badge dots ${small ? 'small' : ''}`} aria-label="Working">
        <i />
        <i />
        <i />
      </span>
    )
  }
  const color = badge === 'needsInput' ? AMBER : badge === 'finished' ? GREEN : RED
  const label = badge === 'needsInput' ? 'Needs you' : badge === 'finished' ? 'Done' : 'Failed'
  return (
    <span className={`island-badge ${small ? 'small' : ''}`} style={{ background: color }} aria-label={label}>
      <Icon name={badge === 'finished' ? 'checkmark' : 'exclamationmark'} size={small ? 5 : 8} weight="black" scaled={false} />
    </span>
  )
}

/** The bot's color dot, its name and what it's doing. */
export function AgentRow({ color, name, subtitle }: { color: string; name: string; subtitle: string }) {
  return (
    <div className="island-agent">
      <span className="island-dot" style={{ background: avatarColor(color) }} />
      <span className="island-agent-name">{name}</span>
      <span className="island-agent-subtitle">{subtitle}</span>
    </div>
  )
}

/** The other bots down the right edge; each opens its conversation. */
export function MiniColumn({ bots, onOpen }: { bots: IslandBot[]; onOpen: (ref: string) => void }) {
  const arrivals = useArrivals(bots.map((b) => b.ref))
  const column = useRef<HTMLDivElement>(null)
  useFlip(column)
  return (
    <div ref={column} className="island-minis">
      {bots.map((b) => (
        <button key={b.ref} data-flip={b.ref} className={`island-mini ${arrivals.has(b.ref) ? 'island-swap' : ''}`} title={b.name} aria-label={`Open ${b.name}`} onClick={() => onOpen(b.ref)}>
          <CharacterAvatar customIcon={b.customIcon} shape={b.shape} color={b.color} size={18} mood={moodOf(b)} dark />
          <StatusDot bot={b} />
        </button>
      ))}
    </div>
  )
}

/** A small dot on a mini character: amber needs you, white working, red failed. */
export function StatusDot({ bot }: { bot: Pick<IslandBot, 'status'> }) {
  const color = bot.status === 'needsInput' ? AMBER : bot.status === 'working' ? '#F5F6F8' : bot.status === 'error' ? RED : null
  return color ? <span className="island-status-dot" style={{ background: color }} /> : null
}

/** The shortcut on an approval button. */
export const KeyHint = ({ keys }: { keys: string }) => <span aria-hidden className="island-key">{keys}</span>

/** A ticker row's icon for the tool's kind. */
export function stepIcon(kind: string) {
  switch (kind) {
    case 'execute':
      return 'terminal'
    case 'edit':
    case 'delete':
    case 'move':
      return 'pencil'
    case 'read':
      return 'doc.text'
    case 'search':
      return 'magnifyingglass'
    case 'fetch':
      return 'globe'
    default:
      return 'ellipsis'
  }
}
