import type { ReactNode } from 'react'
import { useImageZoom } from './use-image-zoom'
import './attachments.css'

/** A picture that can be zoomed and moved (see `useImageZoom`). Its first child is the picture, drawn at fit size. */
export function ZoomableImage({ children, onZoom }: { children: ReactNode; onZoom?: () => void }) {
  const { ref, zoomed, style, handlers } = useImageZoom(onZoom)
  return (
    <div ref={ref} className={`zoom-box${zoomed ? ' zoomed' : ''}`} style={style} onDragStart={(e) => e.preventDefault()} {...handlers}>
      {children}
    </div>
  )
}
