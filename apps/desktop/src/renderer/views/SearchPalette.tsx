import { useEffect, useRef, useState } from 'react'
import { Icon } from '../components/Icon'
import { Sheet } from '../components/Overlay'
import { VStack } from '../components/Stack'
import { Text } from '../components/Text'
import { useTr } from '../lib/use-language'
import { matchItems, type PaletteItem } from './palette-items'
import './search-palette.css'

const isMac = window.codync.platform === 'darwin'
const modifier = isMac ? '⌘' : 'Ctrl+'

/**
 * Grok Bot's search, for the list beside it (bots, threads or workflows): a floating list over the window,
 * arrows/Enter or ⌘1-9 to open one. `label` names the field and `empty` what shows with nothing matching.
 */
export function SearchPalette({ open, items, label, empty, onClose }: { open: boolean; items: PaletteItem[]; label: string; empty: string; onClose: () => void }) {
  return (
    <Sheet open={open} onClose={onClose} width={640} dismissOnDim>
      {open ? <PaletteBody items={items} label={label} empty={empty} onClose={onClose} /> : null}
    </Sheet>
  )
}

function PaletteBody({ items, label, empty, onClose }: { items: PaletteItem[]; label: string; empty: string; onClose: () => void }) {
  const tr = useTr()
  const [query, setQuery] = useState('')
  const [active, setActive] = useState(0)
  const list = useRef<HTMLDivElement>(null)
  const results = matchItems(items, query)
  const pick = (item: PaletteItem | undefined) => {
    if (!item) return
    item.pick()
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
        <input autoFocus value={query} placeholder={tr('Search')} aria-label={tr(label)} spellCheck={false} onChange={(e) => setQuery(e.target.value)} />
      </div>
      <div className="palette-list" ref={list} role="listbox">
        {results.map((item, i) => (
          <button
            key={item.key}
            role="option"
            aria-selected={i === active}
            className={`palette-row ${i === active ? 'active' : ''}`}
            onMouseMove={() => setActive(i)}
            onClick={() => pick(item)}
          >
            {item.leading}
            <VStack as="span" gap={2} grow>
              <Text size="body" tone="text" truncate>{item.title}</Text>
              {item.subtitle ? <Text size="subheadline" tone="secondary" truncate>{item.subtitle}</Text> : null}
            </VStack>
            {i < 9 ? <span className="key-cap palette-key">{`${modifier}${i + 1}`}</span> : null}
          </button>
        ))}
        {results.length === 0 ? <div className="palette-empty">{tr(empty)}</div> : null}
      </div>
    </div>
  )
}

