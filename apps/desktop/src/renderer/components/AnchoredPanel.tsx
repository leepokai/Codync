import { useLayoutEffect, useState, type ReactNode, type RefObject } from 'react'
import { createPortal } from 'react-dom'
import { anchorPlacement, useEscape, usePresence } from './Overlay'

/** A floating panel next to `anchor` that a click anywhere else closes (kit `AnchoredPanel`). */
export function AnchoredPanel({ open, onClose, anchor, children }: { open: boolean; onClose: () => void; anchor: RefObject<HTMLElement | null>; children: ReactNode }) {
  const { mounted, shown } = usePresence(open, 120)
  const [rect, setRect] = useState<DOMRect | null>(null)
  useLayoutEffect(() => {
    if (open && anchor.current) setRect(anchor.current.getBoundingClientRect())
  }, [open, anchor])
  useEscape(open, onClose)
  if (!mounted || !rect) return null
  return createPortal(
    <div className={`anchored ${shown ? 'shown' : ''}`} onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="anchored-panel" style={anchorPlacement(rect).style}>
        {children}
      </div>
    </div>,
    document.body,
  )
}
