import { useEffect, useRef } from 'react'
import { dotGrid, fitRadius } from '../lib/dot-grid'
import { useDark, usePixelRatio, useReduceMotion } from '../lib/theme'
import { ORB_REST, orbCells, orbFrame, type OrbState } from '../lib/thinking-orb'

export type { OrbState }

/** The halftone thinking orb (decorative): working, searching, listening, connecting or shaping. */
export function ThinkingOrb({ state = 'working', size = 16, color = 'var(--text)', animated = true }: { state?: OrbState; size?: number; color?: string; animated?: boolean }) {
  const canvas = useRef<HTMLCanvasElement>(null)
  const reduce = useReduceMotion()
  const dark = useDark()
  const ratio = usePixelRatio()
  useEffect(() => {
    const el = canvas.current
    if (!el) return
    el.width = Math.round(size * ratio)
    el.height = Math.round(size * ratio)
    const ctx = el.getContext('2d')!
    const ink = getComputedStyle(el).color
    // Below 2x the avatars' pixel-snapped grid keeps every dot crisp; Retina gets the exact one.
    const n = orbCells(size), grid = dotGrid(size, ratio, n), grow = grid.pitch / (size / n)
    const running = animated && !reduce
    const epoch = performance.now()
    let frame = 0
    let last = 0
    const draw = (now: number) => {
      if (running && now - last < 1000 / 30) {
        frame = requestAnimationFrame(draw)
        return
      }
      last = now
      const time = ORB_REST[state] + (running ? Math.max(0, now - epoch) / 1000 : 0)
      ctx.setTransform(ratio, 0, 0, ratio, 0, 0)
      ctx.clearRect(0, 0, size, size)
      ctx.fillStyle = ink
      for (const d of orbFrame(state, size, time)) {
        const x = grid.first + d.col * grid.pitch, y = grid.first + d.row * grid.pitch
        ctx.globalAlpha = d.alpha
        ctx.beginPath()
        ctx.arc(x, y, fitRadius(d.radius * grow, x, y, size), 0, Math.PI * 2)
        ctx.fill()
      }
      ctx.globalAlpha = 1
      if (running) frame = requestAnimationFrame(draw)
    }
    frame = requestAnimationFrame(draw)
    return () => cancelAnimationFrame(frame)
  }, [state, size, animated, reduce, color, dark, ratio])
  return <canvas ref={canvas} aria-hidden style={{ width: size, height: size, color, display: 'block', flex: 'none' }} />
}
