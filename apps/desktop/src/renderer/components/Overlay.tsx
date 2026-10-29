import { createContext, useContext, useEffect, useId, useLayoutEffect, useRef, useState, type ReactElement, type ReactNode, type RefObject } from 'react'
import { createPortal } from 'react-dom'
import { reduceMotion } from '../lib/theme'
import { afterExit } from '../lib/exit-delay'
import { useTr } from '../lib/use-language'
import { Icon } from './Icon'
import { HStack, VStack } from './Stack'
import { Text } from './Text'
import './overlay.css'

/** Keeps a layer mounted while it animates away; `shown` drives the CSS transition. */
export function usePresence(open: boolean, exitMs = 300, animateOnMount = true) {
  const [mounted, setMounted] = useState(open)
  const [shown, setShown] = useState(open && !animateOnMount)
  useLayoutEffect(() => {
    if (reduceMotion()) {
      setMounted(open)
      setShown(open)
      return
    }
    if (open) {
      setMounted(true)
      let reveal = 0
      const frame = requestAnimationFrame(() => { reveal = requestAnimationFrame(() => setShown(true)) })
      return () => {
        cancelAnimationFrame(frame)
        cancelAnimationFrame(reveal)
      }
    }
    setShown(false)
    return afterExit(exitMs, () => setMounted(false))
  }, [open, exitMs])
  return { mounted, shown }
}

/** Closes the modal the view is in (kit's `\.dismissModal`). */
export const DismissContext = createContext<() => void>(() => {})
export const useDismiss = () => useContext(DismissContext)

/** Stack depth: only the top layer takes Escape. */
const stack: number[] = []
let nextLayer = 0
export function useEscape(active: boolean, onEscape: () => void) {
  const handler = useRef(onEscape)
  handler.current = onEscape
  useEffect(() => {
    if (!active) return
    const id = ++nextLayer
    stack.push(id)
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && stack[stack.length - 1] === id) {
        e.preventDefault()
        e.stopPropagation()
        handler.current()
      }
    }
    window.addEventListener('keydown', onKey, true)
    return () => {
      window.removeEventListener('keydown', onKey, true)
      stack.splice(stack.indexOf(id), 1)
    }
  }, [active])
}

const FOCUSABLE = 'button:not(:disabled), [href], input:not(:disabled), textarea:not(:disabled), select:not(:disabled), [tabindex]:not([tabindex="-1"])'

/** The open modal's title id, so its `ModalHeader` names the dialog. */
const ModalTitleContext = createContext<string | undefined>(undefined)

/**
 * An open modal holds focus: everything behind it goes inert, focus moves in (to a
 * `data-autofocus` control, else the card itself), and returns to where it was on close.
 */
function useModalFocus(open: boolean, mounted: boolean, layer: RefObject<HTMLElement | null>) {
  useEffect(() => {
    const node = layer.current
    if (!open || !node) return
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null
    const muted = [...document.body.children].filter((el): el is HTMLElement => el instanceof HTMLElement && el !== node && !el.inert)
    for (const el of muted) el.inert = true
    if (!node.contains(document.activeElement)) (node.querySelector<HTMLElement>('[data-autofocus]') ?? node.querySelector<HTMLElement>('[data-modal-card]'))?.focus({ preventScroll: true })
    // Tab cycles inside the top modal instead of leaving the page.
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Tab' || node.inert) return
      const items = [...node.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => el.getClientRects().length > 0 && !el.closest('[inert]'))
      const first = items[0]
      const last = items[items.length - 1]
      if (!first || !last) return
      const active = document.activeElement
      const wrap = e.shiftKey ? active === first || !node.contains(active) : active === last
      if (!wrap) return
      e.preventDefault()
      ;(e.shiftKey ? last : first).focus()
    }
    window.addEventListener('keydown', onKey, true)
    return () => {
      window.removeEventListener('keydown', onKey, true)
      for (const el of muted) el.inert = false
      if (previous?.isConnected) previous.focus({ preventScroll: true })
    }
  }, [open, mounted, layer])
}

/** The Mac modal (`.codyncSheet`): a centered card over a dim; the content sizes it. */
export function Sheet({ open, onClose, width, height, maxHeight, dismissOnDim = false, children }: {
  open: boolean
  onClose: () => void
  /** A click outside the card closes it (transient pickers, not forms). */
  dismissOnDim?: boolean
  width?: number | string
  height?: number | string
  /** Without `height` the card fits its content, up to this (and the window). */
  maxHeight?: number | string
  children: ReactNode
}) {
  const { mounted, shown } = usePresence(open)
  const last = useRef(children)
  const layer = useRef<HTMLDivElement>(null)
  const titleId = useId()
  if (open) last.current = children
  useEscape(open, onClose)
  useModalFocus(open, mounted, layer)
  if (!mounted) return null
  return createPortal(
    <DismissContext.Provider value={onClose}>
      <ModalTitleContext.Provider value={titleId}>
        <div ref={layer} className={`modal-card ${shown ? 'shown' : ''}`} role="dialog" aria-modal aria-labelledby={titleId}>
          <div className="modal-dim" onClick={dismissOnDim ? onClose : undefined} />
          <div className="modal-content" data-modal-card tabIndex={-1} style={{ width, height, maxHeight }}>
            {open ? children : last.current}
          </div>
        </div>
      </ModalTitleContext.Provider>
    </DismissContext.Provider>,
    document.body,
  )
}

export interface DialogAction {
  title: string
  destructive?: boolean
  action: () => void
}

/** A centered card over a dimmed screen with the actions and Cancel (`.codyncDialog`). */
export function Dialog({ open, title, message, actions, cancel = 'Cancel', onClose }: {
  open: boolean
  title: string
  message?: string | null
  actions: DialogAction[]
  cancel?: string | null
  onClose: () => void
}) {
  const { mounted, shown } = usePresence(open, 120)
  const tr = useTr()
  const last = useRef({ title, message, actions })
  const layer = useRef<HTMLDivElement>(null)
  const id = useId()
  if (open) last.current = { title, message, actions }
  useEscape(open && cancel !== null, onClose)
  useModalFocus(open, mounted, layer)
  if (!mounted) return null
  const shownContent = last.current
  return createPortal(
    <div ref={layer} className={`dialog ${shown ? 'shown' : ''}`} role="alertdialog" aria-modal aria-labelledby={`${id}-title`} aria-describedby={shownContent.message ? `${id}-message` : undefined}>
      <div className="dialog-dim" onClick={() => cancel !== null && onClose()} />
      <div className="dialog-card" data-modal-card tabIndex={-1}>
        <div className="dialog-text">
          {/* Callers translate the sentence, then insert names. A second pass would also translate a device named "Done". */}
          <Text as="h2" id={`${id}-title`} size="title3" weight="semibold">{shownContent.title}</Text>
          {shownContent.message ? <Text as="div" id={`${id}-message`} size="body" tone="secondary">{shownContent.message}</Text> : null}
        </div>
        <div className="dialog-buttons">
          {cancel !== null ? (
            <button className="dialog-button" data-autofocus onClick={onClose}>
              {tr(cancel)}
            </button>
          ) : null}
          {shownContent.actions.map((a) => (
            <button
              key={a.title}
              className={`dialog-button ${a.destructive ? 'destructive' : 'prominent'}`}
              onClick={() => {
                onClose()
                a.action()
              }}
            >
              {tr(a.title)}
            </button>
          ))}
        </div>
      </div>
    </div>,
    document.body,
  )
}

/** One row in a Codync menu: an action, or a choice when `selected` is set. */
export interface MenuItem {
  title: string
  icon?: string
  /** A picture in the icon's place (an agent's logo). */
  leading?: ReactNode
  selected?: boolean
  destructive?: boolean
  divider?: boolean
  action: () => void
}

/** A row of emoji above a message's menu (Slack's quick reactions). */
export interface ReactionPick {
  emoji: string[]
  chosen: string[]
  toggle: (emoji: string) => void
}

export function ReactionStrip({ pick, dismiss, size = 26 }: { pick: ReactionPick; dismiss?: () => void; size?: number }) {
  return (
    <HStack align="stretch">
      {pick.emoji.map((emoji) => {
        const chosen = pick.chosen.includes(emoji)
        return (
          <button
            key={emoji}
            className={`reaction-button ${chosen ? 'chosen' : ''}`}
            style={{ width: size, height: size, borderRadius: size * 0.28 }}
            title={chosen ? `Remove ${emoji}` : `React ${emoji}`}
            aria-label={chosen ? `Remove reaction ${emoji}` : `React ${emoji}`}
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
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault()
        setHighlight((h) => (h + (e.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length)
      } else if (e.key === 'Enter' && highlight >= 0) {
        e.preventDefault()
        dismiss()
        items[highlight]?.action()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [items, highlight, dismiss])
  return (
    <div className="menu-panel" role="menu" style={{ maxHeight }}>
      {reactions ? (
        <div className={items.length ? 'menu-reactions' : undefined}>
          <ReactionStrip pick={reactions} dismiss={dismiss} />
        </div>
      ) : null}
      {items.map((item, i) => (
        <div key={`${item.title}-${i}`} role="none">
          {item.divider ? <div className="menu-divider" role="separator" /> : null}
          <button
            className={`menu-row ${item.destructive ? 'destructive' : ''} ${highlight === i ? 'highlight' : ''}`}
            role={item.selected === undefined ? 'menuitem' : 'menuitemradio'}
            aria-checked={item.selected}
            title={tr(item.title)}
            onMouseEnter={() => setHighlight(i)}
            onClick={() => {
              dismiss()
              item.action()
            }}
          >
            {item.leading || item.icon ? (
              <span className="menu-icon">
                {item.leading ?? (item.icon ? <Icon name={item.icon} size={12} /> : null)}
              </span>
            ) : null}
            <span className="menu-title">{tr(item.title)}</span>
            {item.selected !== undefined ? (
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
  if (open) frozen.current = items()
  useLayoutEffect(() => {
    if (!open) return
    if (point) setRect(new DOMRect(point.x, point.y, 0, 0))
    else if (anchor?.current) setRect(anchor.current.getBoundingClientRect())
  }, [open, point, anchor])
  useEscape(open, onClose)
  if (!mounted || !rect) return null
  const { style, width, room } = anchorPlacement(rect)
  return createPortal(
    <div className={`anchored ${shown ? 'shown' : ''}`} onMouseDown={(e) => e.target === e.currentTarget && onClose()} onContextMenu={(e) => { e.preventDefault(); onClose() }}>
      <div className="anchored-panel" style={{ ...style, maxWidth: width }}>
        <MenuPanel items={frozen.current} reactions={reactions} dismiss={onClose} maxHeight={Math.min(420, room)} />
      </div>
    </div>,
    document.body,
  )
}

/** Right-click opens a Codync menu at the pointer (kit's `.contextActions`). */
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
    close: () => setPoint(null),
  }
}

/** A sheet's content: a full-height column on the page color, usually a ModalHeader and a body. */
export function SheetPage({ className, children }: { className?: string; children: ReactNode }) {
  return <div className={`sheet-page${className ? ` ${className}` : ''}`}>{children}</div>
}

/**
 * A modal's title row: an optional icon, the title (and a line under it), optional trailing
 * actions, and a close button that dismisses the modal, or calls `onClose` for a panel that
 * isn't one. `onClose={null}` leaves the close button out (a modal only its own action ends).
 */
export function ModalHeader({ title, subtitle, icon, trailing, onClose, closeTitle = 'Close' }: {
  title: ReactNode
  subtitle?: ReactNode
  /** An SF Symbol name (shown on a tile), or a picture of its own (an agent's logo). */
  icon?: string | ReactElement
  trailing?: ReactNode
  onClose?: (() => void) | null
  closeTitle?: string
}) {
  const dismiss = useDismiss()
  const titleId = useContext(ModalTitleContext)
  return (
    <div className="modal-header">
      {typeof icon === 'string' ? (
        <span className="modal-header-icon" aria-hidden>
          <Icon name={icon} size={15} />
        </span>
      ) : icon}
      <VStack gap={1} className="modal-title">
        <Text as="h2" id={titleId} size="title3" weight="semibold" tone="text" lines={2}>{title}</Text>
        {subtitle !== undefined ? <Text size="callout" tone="secondary" lines={2} title={typeof subtitle === 'string' ? subtitle : undefined}>{subtitle}</Text> : null}
      </VStack>
      {trailing}
      {onClose === null ? null : <IconButtonBase title={closeTitle} icon="xmark" onClick={onClose ?? dismiss} />}
    </div>
  )
}

/** Plain icon button that lights up on hover and press. Icon-only, labelled for VoiceOver. */
export function IconButtonBase({ title, icon, onClick, selected = false, size = 28, disabled, className, buttonRef, menuOpen }: {
  title: string
  icon: string
  onClick?: (e: React.MouseEvent) => void
  selected?: boolean
  size?: number
  disabled?: boolean
  className?: string
  buttonRef?: RefObject<HTMLButtonElement | null>
  /** Set when the button opens a menu: whether that menu is open. */
  menuOpen?: boolean
}) {
  const tr = useTr()
  const label = tr(title)
  return (
    <button
      ref={buttonRef}
      className={`icon-button ${selected ? 'selected' : ''} ${className ?? ''}`}
      style={{ width: size, height: size, borderRadius: size * 0.28 }}
      title={label}
      aria-label={label}
      aria-haspopup={menuOpen === undefined ? undefined : 'menu'}
      aria-expanded={menuOpen}
      disabled={disabled}
      onClick={onClick}
    >
      <Icon name={icon} size={size / 2} weight="medium" />
    </button>
  )
}
