import { useEffect, useRef } from 'react'
import { useDark, usePixelRatio, useReduceMotion, withAlpha } from '../lib/theme'

/** What the dots show this frame: your voice's level (0…1), the other side talking, or work going on. */
export interface LevelSample {
  level: number
  speaking?: boolean
  working?: boolean
}

/** The gap between dots, in points. */
const SPACING = 8

/**
 * A dotted line that follows your voice while listening, ripples while the other side talks,
 * breathes while work goes on, and rests as faint dots otherwise. `sample` is read every frame.
 * `color` null is the text color.
 */
export function LevelDots({ sample, color, width, height = 16, label, className }: {
  sample: () => LevelSample
  color: string | null
  width: number
  height?: number
  label: string
  className?: string
}) {
  const canvas = useRef<HTMLCanvasElement>(null)
  const dark = useDark()
  const reduceMotion = useReduceMotion()
  const scale = usePixelRatio()
  const live = useRef({ sample, color, dark, reduceMotion })
  live.current = { sample, color, dark, reduceMotion }

  useEffect(() => {
    const el = canvas.current
    const ctx = el?.getContext('2d')
    if (!el || !ctx) return
    el.width = width * scale
    el.height = height * scale
    let frame = 0
    const draw = (now: number) => {
      const { sample, color, dark, reduceMotion } = live.current
      const { level, speaking = false, working = false } = sample()
      const t = reduceMotion ? 0 : now / 1000
      const accent = dark ? '#ffffff' : '#000000'
      const tint = speaking ? accent : (color ?? accent)
      ctx.setTransform(scale, 0, 0, scale, 0, 0)
      ctx.clearRect(0, 0, width, height)
      const count = Math.max(1, Math.floor(width / SPACING))
      for (let i = 0; i < count; i++) {
        const x = i * SPACING + SPACING / 2
        const wave = (Math.sin(i * 0.55 - t * 6) + 1) / 2
        const amount = speaking ? 0.5 + 0.5 * wave : level > 0.05 ? Math.max(0.2, level) * (0.45 + 0.55 * wave) : working ? 0.25 * ((Math.sin(t * 2.4) + 1) / 2) : 0.08
        const bar = 4 + amount * (height - 4)
        ctx.fillStyle = withAlpha(tint, 0.5 + 0.5 * amount)
        ctx.beginPath()
        ctx.roundRect(x - 1.25, (height - bar) / 2, 2.5, bar, 1.25)
        ctx.fill()
      }
      frame = requestAnimationFrame(draw)
    }
    frame = requestAnimationFrame(draw)
    return () => cancelAnimationFrame(frame)
  }, [scale, width, height])

  return <canvas ref={canvas} role="img" aria-label={label} className={className} style={{ width, height }} />
}
