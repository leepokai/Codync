import { Fragment, useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type KeyboardEvent, type ReactNode, type RefObject } from 'react'
import { createPortal } from 'react-dom'
import type { Bot } from '@shared/models'
import { AgentIcon } from '../../components/AgentIcon'
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

/**
 * A list over the transcript, above the box: each row's icon, name and what it is. `section` names the group a row
 * comes from, null for the list's own (`ownSection`); while any row comes from elsewhere, a heading starts each group.
 */
function ComposerMenu<T>({ menu, anchor, label, row, section, ownSection = '' }: {
  menu: ComposerMenuState<T>
  anchor: RefObject<HTMLElement | null>
  label: string
  row: (item: T) => { key: string; icon: ReactNode; title: string; detail: string; warning?: boolean }
  section?: (item: T) => string | null
  ownSection?: string
}) {
  const open = menu.shown.length > 0
  const { mounted, shown } = usePresence(open)
  const last = useRef(menu.shown)
  if (open) last.current = menu.shown
  const list = open ? menu.shown : last.current
  const rows = useRef<(HTMLButtonElement | null)[]>([])
  const [pos, setPos] = useState<{ left: number; width: number; bottom: number; maxHeight: number } | null>(null)
  const groupOf = (item: T) => section?.(item) ?? ownSection
  const grouped = section !== undefined && list.some((item) => section(item) !== null)

  // Measure the composer surface against the viewport and any scrolling dock wrapper so the menu
  // stays anchored above it without being clipped by overflow or the top of a short window.
  useLayoutEffect(() => {
    if (!mounted) {
      setPos(null)
      return
    }
    const target = anchor.current
    if (!target) return
    const scroller = target.closest('.composer-dock-scroll')
    const dock = target.closest('.composer-dock')
    const measure = () => {
      const rect = target.getBoundingClientRect()
      const scrollRect = scroller?.getBoundingClientRect()
      const anchorTop = scrollRect ? Math.min(Math.max(rect.top, scrollRect.top), scrollRect.bottom) : rect.top
      const bottom = Math.max(8, window.innerHeight - anchorTop + 8)
      const maxHeight = Math.min(300, Math.max(80, anchorTop - 16))
      setPos({ left: rect.left, width: rect.width, bottom, maxHeight })
    }
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(target)
    if (scroller) observer.observe(scroller)
    if (dock && dock !== scroller) observer.observe(dock)
    window.addEventListener('resize', measure)
    window.addEventListener('scroll', measure, true)
    return () => {
      observer.disconnect()
      window.removeEventListener('resize', measure)
      window.removeEventListener('scroll', measure, true)
    }
  }, [mounted, anchor])

  // Braces: newer Chromium returns a promise from `scrollIntoView`, which an effect mustn't return.
  useEffect(() => {
    void rows.current[menu.index]?.scrollIntoView({ block: 'nearest' })
  }, [menu.index, list])
  if (!mounted || !pos) return null
  return createPortal(
    <div
      className={`composer-menu ${shown ? 'shown' : ''}`}
      inert={!open}
      style={{
        left: pos.left,
        width: pos.width,
        bottom: pos.bottom,
        '--composer-menu-max': `${pos.maxHeight}px`,
      } as CSSProperties}
    >
      <div className="menu-panel" role="listbox" aria-label={label}>
        {list.map((item, i) => {
          const { key, icon, title, detail, warning } = row(item)
          const group = groupOf(item)
          return (
            <Fragment key={key}>
              {grouped && (i === 0 || groupOf(list[i - 1]) !== group) ? (
                <div className="menu-section" role="presentation"><Text size="footnote" tone="secondary" truncate>{group}</Text></div>
              ) : null}
              <button
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
            </Fragment>
          )
        })}
      </div>
    </div>,
    document.body,
  )
}

/**
 * The `/` list: each command's icon (an agent's own command: that agent's logo), name and what it does. While any
 * of the agent's own commands is listed, headings set Codync's (Fast mode, skills) apart from the agent's (its name).
 */
export function CommandMenu({ menu, anchor, agent }: {
  menu: ComposerMenuState<SlashCommand>
  anchor: RefObject<HTMLElement | null>
  agent: { registry: (id: string) => string; name: (id: string) => string }
}) {
  const tr = useTr()
  const section = (c: SlashCommand) => c.action.kind === 'agent' ? agent.name(c.action.agent) : null
  return <ComposerMenu menu={menu} anchor={anchor} label={tr('Commands')} section={section} ownSection="Codync" row={(c) => c.action.kind === 'agent' ? {
    key: `agent:${c.name}`,
    icon: <AgentIcon registry={agent.registry(c.action.agent)} size={12} color="var(--secondary)" />,
    title: c.title,
    detail: c.detail,
  } : {
    key: `${c.action.kind}:${c.name}`,
    icon: <Icon name={c.icon} size={12} color="var(--secondary)" />,
    title: tr(c.title),
    detail: tr(c.detail),
    warning: c.warning,
  }} />
}

/** The `@` list in a group: each member's face, name and description. */
export function MentionMenu({ menu, anchor }: { menu: ComposerMenuState<Bot>; anchor: RefObject<HTMLElement | null> }) {
  const tr = useTr()
  return <ComposerMenu menu={menu} anchor={anchor} label={tr('Members')} row={(m) => ({
    key: m.id,
    icon: <BotAvatar bot={m} size={18} animated={false} />,
    title: m.name,
    detail: m.description,
  })} />
}
