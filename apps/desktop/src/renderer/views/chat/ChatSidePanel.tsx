import { useRef, type ReactNode } from 'react'
import { ResizeHandle } from '../../components/Controls'
import { usePresence } from '../../components/Overlay'
import { useTr } from '../../lib/use-language'

/**
 * The panel beside the conversation. The conversation takes its new width at once (one layout,
 * however long the chat), then the panel slides in or out over it; its edge drags the width. The
 * slide moves `right`, not a transform: window drag regions (the panel's top band) follow layout,
 * and a transform would leave them where the slide started, over the corner buttons. `filled`: the panel covers
 * the conversation (which keeps its layout under it) until it is restored.
 */
export function ChatSidePanel({ open, filled, width, min, max, onResize, children }: {
  open: boolean
  filled: boolean
  width: number
  min: number
  max: number
  onResize: (width: number) => void
  children: ReactNode
}) {
  const tr = useTr()
  // Animate explicit open/close actions without animating the initial mount.
  const { mounted, shown } = usePresence(open, 300, false)
  const last = useRef({ width, children })
  if (open) last.current = { width, children }
  const content = open ? { width, children } : last.current
  // The content plus the edge's hairline.
  const panelWidth = content.width + 1
  return (
    <>
      <div className="side-panel-slot" style={{ width: open ? panelWidth : 0 }} />
      {mounted ? (
        <div className="side-panel" inert={!open} style={{ width: filled ? '100%' : panelWidth, right: shown ? 0 : -panelWidth }}>
          {filled ? null : <ResizeHandle label={tr('Resize panel')} width={content.width} min={min} max={max} edge="leading" onResize={onResize} />}
          <div className="side-panel-content" style={{ width: filled ? '100%' : content.width }}>{content.children}</div>
        </div>
      ) : null}
    </>
  )
}
