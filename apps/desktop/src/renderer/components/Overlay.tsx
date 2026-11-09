import { createContext, useContext, useEffect, useId, useLayoutEffect, useRef, useState, type HTMLAttributes, type ReactElement, type ReactNode, type RefObject } from 'react'
import { createPortal } from 'react-dom'
import { reduceMotion } from '../lib/theme'
import { afterExit } from '../lib/exit-delay'
import { stackModals } from '../lib/modal-stack'
import { useTr } from '../lib/use-language'
import { Icon } from './Icon'
import { VStack } from './Stack'
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

/** Layers in the order they opened: Escape goes to the newest one still live (not inert under the modal on top). */
const stack: { id: number; layer: RefObject<HTMLElement | null> }[] = []
let nextLayer = 0
export function useEscape(active: boolean, onEscape: () => void, layer: RefObject<HTMLElement | null>) {
  const handler = useRef(onEscape)
  handler.current = onEscape
  useEffect(() => {
    if (!active) return
    const id = ++nextLayer
    stack.push({ id, layer })
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && stack.findLast((entry) => !entry.layer.current?.inert)?.id === id) {
        e.preventDefault()
        e.stopPropagation()
        handler.current()
      }
    }
    window.addEventListener('keydown', onKey, true)
    return () => {
      window.removeEventListener('keydown', onKey, true)
      stack.splice(stack.findIndex((entry) => entry.id === id), 1)
    }
  }, [active, layer])
}

const FOCUSABLE = 'button:not(:disabled), [href], input:not(:disabled), textarea:not(:disabled), select:not(:disabled), [tabindex]:not([tabindex="-1"])'

/** The open modal's title id, so its `ModalHeader` names the dialog. */
const ModalTitleContext = createContext<string | undefined>(undefined)

/** The open modals with where focus returns when each closes, and the page layers they made inert. */
const openModals = new Map<HTMLElement, HTMLElement | null>()
const mutedByModals = new WeakSet<HTMLElement>()

/** Applies `stackModals` to the page and moves focus into the modal on top. */
function syncModals() {
  const layers = [...document.body.children].filter((el): el is HTMLElement => el instanceof HTMLElement)
  const { top, mute, unmute } = stackModals(layers.map((el) => ({ el, z: Number(getComputedStyle(el).zIndex) || 0, modal: openModals.has(el), inert: el.inert, muted: mutedByModals.has(el) })))
  for (const el of mute) {
    el.inert = true
    mutedByModals.add(el)
  }
  for (const el of unmute) {
    el.inert = false
    mutedByModals.delete(el)
  }
  if (top && !top.contains(document.activeElement)) (top.querySelector<HTMLElement>('[data-autofocus]') ?? top.querySelector<HTMLElement>('[data-modal-card]'))?.focus({ preventScroll: true })
}

/**
 * An open modal holds focus: everything behind it goes inert, focus moves in (to a
 * `data-autofocus` control, else the card itself), and returns to where it was on close.
 */
function useModalFocus(open: boolean, mounted: boolean, layer: RefObject<HTMLElement | null>) {
  useEffect(() => {
    const node = layer.current
    if (!open || !node) return
    openModals.set(node, document.activeElement instanceof HTMLElement ? document.activeElement : null)
    syncModals()
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
      const previous = openModals.get(node)
      openModals.delete(node)
      syncModals()
      if (previous?.isConnected && !previous.closest('[inert]')) {
        previous.focus({ preventScroll: true })
        return
      }
      // Its opener is under a modal still open: the modal opened from this one returns there instead.
      for (const [other, theirs] of openModals) if (theirs && node.contains(theirs)) openModals.set(other, previous ?? null)
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
  useEscape(open, onClose, layer)
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
  disabled?: boolean
  action: () => void
}

/** A centered card over a dimmed screen with the actions and Cancel (`.codyncDialog`). */
export function Dialog({ open, title, message, actions, cancel = 'Cancel', onCancel, onClose, children }: {
  open: boolean
  title: string
  message?: string | null
  actions: DialogAction[]
  cancel?: string | null
  /** Closed without an action; `onClose` follows, as it does after an action. */
  onCancel?: () => void
  onClose: () => void
  /** A field between the text and the buttons (a name to type); `data-autofocus` on it takes the focus. */
  children?: ReactNode
}) {
  const { mounted, shown } = usePresence(open, 120)
  const tr = useTr()
  const last = useRef({ title, message, actions, children })
  const layer = useRef<HTMLDivElement>(null)
  const id = useId()
  if (open) last.current = { title, message, actions, children }
  const dismiss = () => {
    onCancel?.()
    onClose()
  }
  useEscape(open && cancel !== null, dismiss, layer)
  useModalFocus(open, mounted, layer)
  if (!mounted) return null
  const shownContent = last.current
  return createPortal(
    <div ref={layer} className={`dialog ${shown ? 'shown' : ''}`} role="alertdialog" aria-modal aria-labelledby={`${id}-title`} aria-describedby={shownContent.message ? `${id}-message` : undefined}>
      <div className="dialog-dim" onClick={() => cancel !== null && dismiss()} />
      <div className="dialog-card" data-modal-card tabIndex={-1}>
        <div className="dialog-text">
          {/* Callers translate the sentence, then insert names. A second pass would also translate a device named "Done". */}
          <Text as="h2" id={`${id}-title`} size="title3" weight="semibold">{shownContent.title}</Text>
          {shownContent.message ? <Text as="div" id={`${id}-message`} size="body" tone="secondary">{shownContent.message}</Text> : null}
        </div>
        {shownContent.children}
        <div className="dialog-buttons">
          {cancel !== null ? (
            <button className="dialog-button" data-autofocus onClick={dismiss}>
              {tr(cancel)}
            </button>
          ) : null}
          {shownContent.actions.map((a) => (
            <button
              key={a.title}
              className={`dialog-button ${a.destructive ? 'destructive' : 'prominent'}`}
              disabled={a.disabled}
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

/** A `Dialog`'s text box, focused as it opens: Return, once the IME has finished composing, runs `onSubmit`. */
export function DialogField({ value, onChange, onSubmit, placeholder, label, inputMode }: {
  value: string
  onChange: (value: string) => void
  onSubmit: () => void
  placeholder: string
  label: string
  inputMode?: HTMLAttributes<HTMLInputElement>['inputMode']
}) {
  return (
    <input className="dialog-field" data-autofocus value={value} placeholder={placeholder} aria-label={label} inputMode={inputMode} spellCheck={false}
      onChange={(e) => onChange(e.target.value)}
      onKeyDown={(e) => {
        if (e.key !== 'Enter' || e.nativeEvent.isComposing || e.keyCode === 229) return
        e.preventDefault()
        onSubmit()
      }} />
  )
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
export function IconButtonBase({ title, icon, onClick, selected = false, size = 28, disabled, className, buttonRef, menuOpen, tabIndex }: {
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
  /** -1 for a pointer-only button whose action its container offers to the keyboard. */
  tabIndex?: number
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
      tabIndex={tabIndex}
    >
      <Icon name={icon} size={size / 2} weight="medium" />
    </button>
  )
}
