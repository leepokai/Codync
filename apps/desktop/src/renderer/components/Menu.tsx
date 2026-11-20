import { useEffect, useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from 'react'
import { createPortal } from 'react-dom'
import { useTr } from '../lib/use-language'
import { Icon } from './Icon'
import { useEscape, usePresence } from './Overlay'
import { HStack } from './Stack'
import { Text } from './Text'

/** What every row in a Codync menu has. */
interface MenuRow {
  title: string
  /** Secondary context below the main label. */
  detail?: string
  icon?: string
  /** A picture in the icon's place (an agent's logo). */
  leading?: ReactNode
  selected?: boolean
  destructive?: boolean
  divider?: boolean
  /** The title is a name the user gave (a section): shown as typed, never translated. */
  verbatim?: boolean
  /** Picking it leaves the menu open, for choices made several at a time (iOS `.menuActionDismissBehavior(.disabled)`). */
  keepsOpen?: boolean
  /** A line that only says something: dimmed, and can't be picked. */
  disabled?: boolean
}

/**
 * One row in a Codync menu: an action (a choice when `selected` is set), or the rows of a submenu, one level deep. An
 * anchored menu opens them in its place behind a back row ("Set a size ›"), the roster's right-click menu beside it.
 */
export type MenuItem = MenuRow & ({ action: () => void; submenu?: undefined } | { submenu: MenuItem[]; action?: undefined })

/** A row of emoji above a message's menu (Slack's quick reactions). */
export interface ReactionPick {
  emoji: string[]
  chosen: string[]
  toggle: (emoji: string) => void
}

export function ReactionStrip({ pick, dismiss, size = 26 }: { pick: ReactionPick; dismiss?: () => void; size?: number }) {
  const tr = useTr()
  return (
    <HStack align="stretch">
      {pick.emoji.map((emoji) => {
        const chosen = pick.chosen.includes(emoji)
        return (
          <button
            key={emoji}
            className={`reaction-button ${chosen ? 'chosen' : ''}`}
            style={{ width: size, height: size, borderRadius: size * 0.28 }}
            title={chosen ? tr('Remove {emoji}').replaceAll('{emoji}', () => emoji) : tr('React {emoji}').replaceAll('{emoji}', () => emoji)}
            aria-label={chosen ? tr('Remove reaction {emoji}').replaceAll('{emoji}', () => emoji) : tr('React {emoji}').replaceAll('{emoji}', () => emoji)}
            onClick={() => {
              dismiss?.()
              pick.toggle(emoji)
            }}
          >
            <Text size={14}>{emoji}</Text>
          </button>
        )
      })}
    </HStack>
  )
}

/** The floating panel of menu rows. */
export function MenuPanel({ items, reactions, dismiss, maxHeight }: { items: MenuItem[]; reactions?: ReactionPick; dismiss: () => void; maxHeight?: number }) {
  const tr = useTr()
  const [highlight, setHighlight] = useState(-1)
  const [opened, setOpened] = useState<MenuItem | null>(null)
  const panel = useRef<HTMLDivElement>(null)
  const show = (sub: MenuItem | null) => {
    setOpened(sub)
    setHighlight(-1)
  }
  const rows: MenuItem[] = opened
    ? [{ title: opened.title, verbatim: opened.verbatim, icon: 'chevron.left', keepsOpen: true, action: () => show(null) }, ...(opened.submenu ?? [])]
    : items
  const pick = (item: MenuItem) => {
    if (item.disabled) return
    if (item.submenu) return show(item)
    if (!item.keepsOpen) dismiss()
    item.action()
  }
  // A long submenu opens at its checked row.
  useLayoutEffect(() => {
    if (opened) panel.current?.querySelector('[aria-checked="true"]')?.scrollIntoView({ block: 'center' })
  }, [opened])
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault()
        setHighlight((h) => (h + (e.key === 'ArrowDown' ? 1 : -1) + rows.length) % rows.length)
      } else if (e.key === 'Enter' && highlight >= 0 && rows[highlight]) {
        e.preventDefault()
        pick(rows[highlight])
      } else if (e.key === 'ArrowLeft' && opened) {
        e.preventDefault()
        show(null)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [rows, highlight, dismiss, opened])
  return (
    <div ref={panel} className="menu-panel" role="menu" style={{ maxHeight }}>
      {reactions && !opened ? (
        <div className={items.length ? 'menu-reactions' : undefined}>
          <ReactionStrip pick={reactions} dismiss={dismiss} />
        </div>
      ) : null}
      {rows.map((item, i) => (
        <div key={`${opened?.title ?? ''}-${item.title}-${i}`} role="none">
          {item.divider ? <div className="menu-divider" role="separator" /> : null}
          <button
            className={`menu-row ${item.destructive ? 'destructive' : ''} ${highlight === i ? 'highlight' : ''}`}
            role={item.selected === undefined ? 'menuitem' : 'menuitemradio'}
            aria-checked={item.selected}
            aria-haspopup={item.submenu ? 'menu' : undefined}
            disabled={item.disabled}
            title={item.verbatim ? item.title : tr(item.title)}
            onMouseEnter={() => setHighlight(i)}
            onClick={() => pick(item)}
          >
            {item.leading || item.icon ? (
              <span className="menu-icon">
                {item.leading ?? (item.icon ? <Icon name={item.icon} size={12} /> : null)}
              </span>
            ) : null}
            <span className={`menu-title${item.detail ? ' with-detail' : ''}`}>
              <span>{item.verbatim ? item.title : tr(item.title)}</span>
              {item.detail && <span className="menu-detail">{tr(item.detail)}</span>}
            </span>
            {item.submenu ? (
              <Icon name="chevron.right" size={10} weight="semibold" color="var(--secondary)" />
            ) : item.selected !== undefined ? (
              <span className={`check-slot${item.selected ? ' on' : ''}`}>
                <Icon name="checkmark" size={10} weight="semibold" />
              </span>
            ) : null}
          </button>
        </div>
      ))}
    </div>
  )
}

/**
 * Where a panel opens beside `rect`: below it or above (whichever has more room), aligned to the
 * anchor's leading or trailing edge and kept on screen. `room` is the height available.
 */
export function anchorPlacement(rect: DOMRect) {
  const W = window.innerWidth
  const H = window.innerHeight
  const roomBelow = Math.max(0, H - rect.bottom - 14)
  const roomAbove = Math.max(0, rect.top - 14)
  const below = roomBelow >= roomAbove
  const leading = rect.left + rect.width / 2 < W * 0.6
  const width = Math.min(320, W - 16)
  const inset = Math.max(8, W - width - 8)
  const style: React.CSSProperties = {
    [below ? 'top' : 'bottom']: below ? rect.bottom + 6 : H - rect.top + 6,
    ...(leading ? { left: Math.min(Math.max(8, rect.left), inset) } : { right: Math.min(Math.max(8, W - rect.right), inset) }),
    transformOrigin: `${below ? 'top' : 'bottom'} ${leading ? 'left' : 'right'}`,
  }
  return { style, width, room: below ? roomBelow : roomAbove }
}

/**
 * Presents `MenuPanel` next to `anchor` (or at `point`, for right-clicks), flipping to stay on
 * screen; a click anywhere else closes it (`.codyncMenu` / `.contextActions`).
 */
export function AnchoredMenu({ open, onClose, anchor, point, items, reactions }: {
  open: boolean
  onClose: () => void
  anchor?: RefObject<HTMLElement | null>
  point?: { x: number; y: number } | null
  items: () => MenuItem[]
  reactions?: ReactionPick
}) {
  const { mounted, shown } = usePresence(open, 120)
  const [rect, setRect] = useState<DOMRect | null>(null)
  const frozen = useRef<MenuItem[]>([])
  const layer = useRef<HTMLDivElement>(null)
  if (open) frozen.current = items()
  useLayoutEffect(() => {
    if (!open) return
    if (point) setRect(new DOMRect(point.x, point.y, 0, 0))
    else if (anchor?.current) setRect(anchor.current.getBoundingClientRect())
  }, [open, point, anchor])
  useEscape(open, onClose, layer)
  if (!mounted || !rect) return null
  const { style, width, room } = anchorPlacement(rect)
  return createPortal(
    <div ref={layer} className={`anchored ${shown ? 'shown' : ''}`} onMouseDown={(e) => e.target === e.currentTarget && onClose()} onContextMenu={(e) => { e.preventDefault(); e.stopPropagation(); onClose() }}>
      <div className="anchored-panel" style={{ ...style, maxWidth: width }}>
        <MenuPanel items={frozen.current} reactions={reactions} dismiss={onClose} maxHeight={Math.min(420, room)} />
      </div>
    </div>,
    document.body,
  )
}

/** Right-click opens a Codync menu at the pointer (kit's `.contextActions`); `openAt` opens it from the keyboard. */
export function useContextMenu() {
  const [point, setPoint] = useState<{ x: number; y: number } | null>(null)
  return {
    point,
    open: point !== null,
    onContextMenu: (e: React.MouseEvent) => {
      e.preventDefault()
      e.stopPropagation()
      setPoint({ x: e.clientX, y: e.clientY })
    },
    openAt: setPoint,
    close: () => setPoint(null),
  }
}
