import { useEffect, useRef, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import { useEscape } from './Overlay'

/** Idea 18: Latches a value while a slide-over drawer is animating closed so its contents never flash empty. */
export function useLatchedWhileClosing<T>(value: T | null): T | null {
  const latchedRef = useRef<T | null>(value)
  if (value !== null) {
    latchedRef.current = value
  }
  return value ?? latchedRef.current
}

/** Idea 18: Persistent slide-over drawer with spring physics and Escape handling. */
export function SlideShell({
  open,
  onClose,
  ariaLabel,
  width,
  children,
}: {
  open: boolean
  onClose: () => void
  ariaLabel: string
  width?: number | string
  children: ReactNode
}) {
  const layer = useRef<HTMLElement>(null)
  useEscape(open, onClose, layer)

  useEffect(() => {
    if (!open) return
    const prev = document.body.style.overflow
    document.body.style.overflow = 'hidden'
    return () => {
      document.body.style.overflow = prev
    }
  }, [open])

  return createPortal(
    <>
      <div
        aria-hidden
        onClick={onClose}
        className={`t-slide-backdrop ${open ? 'is-open' : ''}`}
      />
      <aside
        ref={layer}
        role="dialog"
        aria-modal={open}
        aria-label={ariaLabel}
        aria-hidden={!open}
        style={width ? { width } : undefined}
        className={`t-slide-panel ${open ? 'is-open' : ''}`}
      >
        {children}
      </aside>
    </>,
    document.body,
  )
}
