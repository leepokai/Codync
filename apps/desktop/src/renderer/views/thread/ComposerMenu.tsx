import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from 'react'
import type { Bot } from '@shared/models'
import { BotAvatar } from '../../components/Avatar'
import { Icon } from '../../components/Icon'
import { usePresence } from '../../components/Overlay'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import type { SlashCommand } from './slash-commands'

interface ComposerMenuState<T> {
  shown: T[]
  index: number
  hover: (index: number) => void
  pick: (item: T) => void
}

/**
 * The highlight and keys of a list above the box (`/` commands, `@` members): ↑/↓ move, Tab or Return picks,
 * Escape hides the list until the draft changes. `returnPicks` lets Return fall through to sending for a row.
 * Both lists are for the query at the end of the draft, so with the caret elsewhere or text selected
 * every key but Escape edits the draft and Return sends.
 */
export function useComposerMenu<T>(draft: string, items: T[], pick: (item: T) => void, returnPicks: (item: T, draft: string) => boolean = () => true) {
  const [highlight, setHighlight] = useState({ draft: '', index: 0 })
  const [dismissed, setDismissed] = useState<string | null>(null)
  if (dismissed !== null && dismissed !== draft) setDismissed(null)
  const shown = dismissed === draft ? [] : items
  const index = highlight.draft === draft ? Math.min(highlight.index, shown.length - 1) : 0

  /** Handles a key in the field while the list is up; returns whether it did. */
  const onKey = (e: KeyboardEvent<HTMLTextAreaElement>, composing: boolean): boolean => {
    const picked = shown[index]
    if (!picked || composing || e.altKey || e.metaKey || e.ctrlKey) return false
    const { selectionStart, selectionEnd } = e.currentTarget
    if ((selectionStart !== draft.length || selectionEnd !== draft.length) && e.key !== 'Escape') return false
    const step = e.key === 'ArrowDown' ? 1 : e.key === 'ArrowUp' ? -1 : 0
    const picks = (e.key === 'Tab' && !e.shiftKey) || (e.key === 'Enter' && !e.shiftKey && returnPicks(picked, draft))
    if (!step && !picks && e.key !== 'Escape') return false
    e.preventDefault()
    e.stopPropagation()
    if (step) setHighlight({ draft, index: (index + step + shown.length) % shown.length })
    else if (picks) pick(picked)
    else setDismissed(draft)
    return true
  }

  return { shown, index, hover: (i: number) => setHighlight({ draft, index: i }), pick, onKey }
}

/** A list over the transcript, above the box: each row's icon, name and what it is. */
function ComposerMenu<T>({ menu, label, row }: {
  menu: ComposerMenuState<T>
  label: string
  row: (item: T) => { key: string; icon: ReactNode; title: string; detail: string; warning?: boolean }
}) {
  const open = menu.shown.length > 0
  const { mounted, shown } = usePresence(open)
  const last = useRef(menu.shown)
  if (open) last.current = menu.shown
  const list = open ? menu.shown : last.current
  const rows = useRef<(HTMLButtonElement | null)[]>([])
  // Braces: newer Chromium returns a promise from `scrollIntoView`, which an effect mustn't return.
  useEffect(() => {
    void rows.current[menu.index]?.scrollIntoView({ block: 'nearest' })
  }, [menu.index, list])
  if (!mounted) return null
  return (
    <div className={`composer-menu ${shown ? 'shown' : ''}`} inert={!open}>
      <div className="menu-panel" role="listbox" aria-label={label}>
        {list.map((item, i) => {
          const { key, icon, title, detail, warning } = row(item)
          return (
            <button
              key={key}
              ref={(el) => { rows.current[i] = el }}
              role="option"
              aria-selected={i === menu.index}
              className={`menu-row ${i === menu.index ? 'highlight' : ''}`}
              title={detail || title}
              onMouseEnter={() => menu.hover(i)}
              // Keep the caret in the field.
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => menu.pick(item)}
            >
              <span className="menu-icon">{icon}</span>
              <Text truncate>{title}</Text>
              <Text truncate grow tone={warning ? 'attention' : 'secondary'}>{detail}</Text>
            </button>
          )
        })}
      </div>
    </div>
  )
}

/** The `/` list: each command's icon, name and what it does. */
export function CommandMenu({ menu }: { menu: ComposerMenuState<SlashCommand> }) {
  const tr = useTr()
  return <ComposerMenu menu={menu} label={tr('Commands')} row={(c) => ({
    key: `${c.action.kind}:${c.name}`,
    icon: <Icon name={c.icon} size={12} color="var(--secondary)" />,
    title: tr(c.title),
    detail: tr(c.detail),
    warning: c.warning,
  })} />
}

/** The `@` list in a group: each member's face, name and description. */
export function MentionMenu({ menu }: { menu: ComposerMenuState<Bot> }) {
  const tr = useTr()
  return <ComposerMenu menu={menu} label={tr('Members')} row={(m) => ({
    key: m.id,
    icon: <BotAvatar bot={m} size={18} animated={false} />,
    title: m.name,
    detail: m.description,
  })} />
}
