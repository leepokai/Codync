import { useEffect, useRef, useState } from 'react'
import { isGroup, type Bot } from '@shared/models'
import { AvatarWithStatus } from '../components/Avatar'
import { Icon } from '../components/Icon'
import { Sheet } from '../components/Overlay'
import { VStack } from '../components/Stack'
import { Text } from '../components/Text'
import { refKey, type BotRef, type RosterItem } from '../store/app-model'
import './search-palette.css'

const isMac = window.codync.platform === 'darwin'
const modifier = isMac ? '⌘' : 'Ctrl+'

/** Bots matching the query by name, description or last message; all bots for an empty query. */
export function matchBots(items: RosterItem[], query: string): RosterItem[] {
  const q = query.trim().toLowerCase()
  if (!q) return items
  return items.filter(({ bot }) => [bot.name, bot.description, bot.lastMessage ?? ''].some((s) => s.toLowerCase().includes(q)))
}

/** Grok Bot's bot search: a floating list over the window, arrows/Enter or ⌘1-9 to open a bot. */
export function SearchPalette({ open, items, onPick, onClose }: { open: boolean; items: RosterItem[]; onPick: (ref: BotRef) => void; onClose: () => void }) {
  return (
    <Sheet open={open} onClose={onClose} width={640} dismissOnDim>
      {open ? <PaletteBody items={items} onPick={onPick} onClose={onClose} /> : null}
    </Sheet>
  )
}

function PaletteBody({ items, onPick, onClose }: { items: RosterItem[]; onPick: (ref: BotRef) => void; onClose: () => void }) {
  const [query, setQuery] = useState('')
  const [active, setActive] = useState(0)
  const list = useRef<HTMLDivElement>(null)
  const results = matchBots(items, query)
  const pick = (item: RosterItem | undefined) => {
    if (!item) return
    onPick(item.ref)
    onClose()
  }

  useEffect(() => setActive(0), [query])
  useEffect(() => {
    list.current?.children[active]?.scrollIntoView({ block: 'nearest' })
  }, [active])

  const onKey = (e: React.KeyboardEvent) => {
    if (e.nativeEvent.isComposing) return
    const digit = Number(e.key)
    if ((isMac ? e.metaKey : e.ctrlKey) && digit >= 1 && digit <= 9) {
      e.preventDefault()
      pick(results[digit - 1])
    } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault()
      const step = e.key === 'ArrowDown' ? 1 : -1
      setActive((i) => (results.length ? (i + step + results.length) % results.length : 0))
    } else if (e.key === 'Enter') {
      e.preventDefault()
      pick(results[active])
    }
  }

  return (
    <div className="palette" onKeyDown={onKey}>
      <div className="palette-field">
        <Icon name="magnifyingglass" size={15} color="var(--secondary)" />
        <input autoFocus value={query} placeholder="Search" aria-label="Search bots" spellCheck={false} onChange={(e) => setQuery(e.target.value)} />
      </div>
      <div className="palette-list" ref={list} role="listbox">
        {results.map((item, i) => (
          <button
            key={refKey(item.ref)}
            role="option"
            aria-selected={i === active}
            className={`palette-row ${i === active ? 'active' : ''}`}
            onMouseMove={() => setActive(i)}
            onClick={() => pick(item)}
          >
            <AvatarWithStatus bot={item.bot} members={isGroup(item.bot) ? item.store.members(item.bot) : []} size={32} />
            <VStack as="span" gap={2} grow>
              <Text size="body" tone="text" truncate>{item.bot.name}</Text>
              {subtitle(item) ? <Text size="subheadline" tone="secondary" truncate>{subtitle(item)}</Text> : null}
            </VStack>
            {i < 9 ? <span className="key-cap palette-key">{`${modifier}${i + 1}`}</span> : null}
          </button>
        ))}
        {results.length === 0 ? <div className="palette-empty">No matching bots</div> : null}
      </div>
    </div>
  )
}

/** What the bot is for; a group lists its members. */
function subtitle({ bot, store }: { bot: Bot; store: RosterItem['store'] }) {
  if (!isGroup(bot) || bot.description) return bot.description
  const names = store.members(bot).map((m) => m.name).join(', ')
  return names === bot.name ? '' : names
}
