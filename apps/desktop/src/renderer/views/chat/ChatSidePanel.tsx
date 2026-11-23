import { useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from 'react'
import { ResizeHandle } from '../../components/Controls'
import { usePresence } from '../../components/Overlay'
import { useTr } from '../../lib/use-language'
import { fitPanel, fitsBeside, PANEL_MIN } from './side-panel-width'

/**
 * The panel beside the conversation. The conversation takes its new width at once (one layout,
 * however long the chat), then the panel slides in or out over it; its edge drags the width. The
 * slide moves `right`, not a transform: window drag regions (the panel's top band) follow layout,
 * and a transform would leave them where the slide started, over the corner buttons. `filled`: the panel covers
 * the conversation (which keeps its layout under it) until it is restored.
 */
export function ChatSidePanel({ open, filled, width, max, onResize, children }: {
  open: boolean
  filled: boolean
  width: number
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
          {filled ? null : <ResizeHandle label={tr('Resize panel')} width={content.width} min={PANEL_MIN} max={max} edge="leading" onResize={onResize} />}
          <div className="side-panel-content" style={{ width: filled ? '100%' : content.width }}>{content.children}</div>
        </div>
      ) : null}
    </>
  )
}

/**
 * The panel's width beside the view `view` measures, which keeps `keep` of its width. The width is measured, not
 * rendered: a resize re-renders the view only when what the panel shows changes. Not `beside`: the panel covers the view.
 */
export function useSidePanelWidth(view: RefObject<HTMLElement | null>, keep: number, wanted: number) {
  const room = useRef(1200)
  const want = useRef(wanted)
  want.current = wanted
  const [, setShown] = useState('')
  useLayoutEffect(() => {
    const el = view.current
    if (!el) return
    const observer = new ResizeObserver(([e]) => {
      room.current = e!.contentRect.width
      setShown(`${fitsBeside(room.current, keep)} ${fitPanel(want.current, room.current, keep)}`)
    })
    observer.observe(el)
    return () => observer.disconnect()
  }, [view, keep])
  const fit = (w: number) => fitPanel(w, room.current, keep)
  return { beside: fitsBeside(room.current, keep), width: fit(wanted), max: fit(Infinity), fit }
}
