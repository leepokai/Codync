import { createContext, useContext, useEffect, useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from 'react'
import { createPortal } from 'react-dom'
import { font } from '../lib/fonts'
import { reduceMotion } from '../lib/theme'
import { Icon } from './Icon'
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
    const timer = setTimeout(() => setMounted(false), reduceMotion() ? 0 : exitMs)
    return () => clearTimeout(timer)
  }, [open, exitMs])
  return { mounted, shown }
}

/** Closes the modal the view is in (kit's `\.dismissModal`). */
export const DismissContext = createContext<() => void>(() => {})
export const useDismiss = () => useContext(DismissContext)

/** Stack depth: only the top layer takes Escape. */
const stack: number[] = []
let nextLayer = 0
function useEscape(active: boolean, onEscape: () => void) {
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

/** The Mac modal (`.codyncSheet`): a centered card over a dim; the content sizes it. */
export function Sheet({ open, onClose, width, height, dismissOnDim = false, children }: {
  open: boolean
  onClose: () => void
  /** A click outside the card closes it (transient pickers, not forms). */
  dismissOnDim?: boolean
  width?: number | string
  height?: number | string
  children: ReactNode
}) {
  const { mounted, shown } = usePresence(open)
  const last = useRef(children)
  if (open) last.current = children
  useEscape(open, onClose)
  if (!mounted) return null
  return createPortal(
    <DismissContext.Provider value={onClose}>
      <div className={`modal-card ${shown ? 'shown' : ''}`} role="dialog" aria-modal>
        <div className="modal-dim" onClick={dismissOnDim ? onClose : undefined} />
        <div className="modal-content" style={{ width, height, maxWidth: 'calc(100vw - 48px)', maxHeight: 'calc(100vh - 48px)' }}>
          {open ? children : last.current}
        </div>
      </div>
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
  const last = useRef({ title, message, actions })
  if (open) last.current = { title, message, actions }
  useEscape(open && cancel !== null, onClose)
  if (!mounted) return null
  const shownContent = last.current
  return createPortal(
    <div className={`dialog ${shown ? 'shown' : ''}`} role="alertdialog" aria-modal>
      <div className="dialog-dim" onClick={() => cancel !== null && onClose()} />
      <div className="dialog-card">
        <div className="dialog-text">
          <div style={font('headline')}>{shownContent.title}</div>
          {shownContent.message ? <div style={{ ...font('compactSecondary'), color: 'var(--secondary)' }}>{shownContent.message}</div> : null}
        </div>
        <div className="dialog-buttons">
          {shownContent.actions.map((a) => (
            <button
              key={a.title}
              className={`dialog-button ${a.destructive ? 'destructive' : 'prominent'}`}
              onClick={() => {
                onClose()
                a.action()
              }}
            >
              {a.title}
            </button>
          ))}
          {cancel !== null ? (
            <button className="dialog-button" onClick={onClose}>
              {cancel}
            </button>
          ) : null}
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
    <div style={{ display: 'flex' }}>
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
            <span style={font(14)}>{emoji}</span>
          </button>
        )
      })}
    </div>
  )
}

/** The floating panel of menu rows. */
export function MenuPanel({ items, reactions, dismiss, maxHeight }: { items: MenuItem[]; reactions?: ReactionPick; dismiss: () => void; maxHeight?: number }) {
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
    <div className="menu-panel" style={{ maxHeight }}>
      {reactions ? (
        <div style={{ paddingBottom: items.length ? 4 : 0 }}>
          <ReactionStrip pick={reactions} dismiss={dismiss} />
        </div>
      ) : null}
      {items.map((item, i) => (
        <div key={`${item.title}-${i}`}>
          {item.divider ? <div className="menu-divider" /> : null}
          <button
            className={`menu-row ${item.destructive ? 'destructive' : ''} ${highlight === i ? 'highlight' : ''}`}
            title={item.title}
            onMouseEnter={() => setHighlight(i)}
            onClick={() => {
              dismiss()
              item.action()
            }}
          >
            {item.icon ? (
              <span className="menu-icon">
                <Icon name={item.icon} size={12} />
              </span>
            ) : null}
            <span className="menu-title">{item.title}</span>
            {item.selected !== undefined ? (
              <span style={{ opacity: item.selected ? 1 : 0, display: 'flex' }}>
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
  const W = window.innerWidth
  const H = window.innerHeight
  const roomBelow = Math.max(0, H - rect.bottom - 14)
  const roomAbove = Math.max(0, rect.top - 14)
  const below = roomBelow >= roomAbove
  const leading = rect.left + rect.width / 2 < W * 0.6
  const width = Math.min(320, W - 16)
  const inset = Math.max(8, W - width - 8)
  const style: React.CSSProperties = {
    position: 'absolute',
    [below ? 'top' : 'bottom']: below ? rect.bottom + 6 : H - rect.top + 6,
    ...(leading ? { left: Math.min(Math.max(8, rect.left), inset) } : { right: Math.min(Math.max(8, W - rect.right), inset) }),
    maxWidth: width,
    transformOrigin: `${below ? 'top' : 'bottom'} ${leading ? 'left' : 'right'}`,
  }
  return createPortal(
    <div className={`anchored ${shown ? 'shown' : ''}`} onMouseDown={(e) => e.target === e.currentTarget && onClose()} onContextMenu={(e) => { e.preventDefault(); onClose() }}>
      <div className="anchored-panel" style={style}>
        <MenuPanel items={frozen.current} reactions={reactions} dismiss={onClose} maxHeight={Math.min(420, below ? roomBelow : roomAbove)} />
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

/** A modal's title row: title, optional trailing actions, and a close button. */
export function ModalHeader({ title, trailing }: { title: ReactNode; trailing?: ReactNode }) {
  const dismiss = useDismiss()
  return (
    <div className="modal-header">
      <div style={{ ...font('compactBody', 'semibold'), flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{title}</div>
      {trailing}
      <IconButtonBase title="Close" icon="xmark" onClick={dismiss} />
    </div>
  )
}

/** Plain icon button that lights up on hover and press. Icon-only, labelled for VoiceOver. */
export function IconButtonBase({ title, icon, onClick, selected = false, size = 28, disabled, className, buttonRef }: {
  title: string
  icon: string
  onClick?: (e: React.MouseEvent) => void
  selected?: boolean
  size?: number
  disabled?: boolean
  className?: string
  buttonRef?: RefObject<HTMLButtonElement | null>
}) {
  return (
    <button
      ref={buttonRef}
      className={`icon-button ${selected ? 'selected' : ''} ${className ?? ''}`}
      style={{ width: size, height: size, borderRadius: size * 0.28 }}
      title={title}
      aria-label={title}
      disabled={disabled}
      onClick={onClick}
    >
      <Icon name={icon} size={size / 2} weight="medium" />
    </button>
  )
}
