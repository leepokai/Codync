import { useEffect, useRef } from 'react'
import { useDark, usePixelRatio, useReduceMotion, withAlpha } from '../lib/theme'

/** What the dots show this frame: your voice's level (0…1), the other side talking, work going on, or a call connecting. */
export interface LevelSample {
  level: number
  speaking?: boolean
  working?: boolean
  connecting?: boolean
}

/** The gap between dots, in points. */
const SPACING = 8
/** Seconds for the connecting segment's trip there and back. */
const PERIOD = 2.4

/**
 * A dotted line that follows your voice while listening, ripples while the other side talks,
 * breathes while work goes on, and rests as faint dots otherwise. While a call connects it shows an
 * indeterminate progress line instead (a short segment gliding back and forth along a faint track, resting
 * at the left with reduced motion), and fades back to the dots. `sample` is read every frame.
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
    /** 1 while connecting, 0 for the dots; eased between them for the fade. */
    let mix = 0
    let last = 0
    const draw = (now: number) => {
      const { sample, color, dark, reduceMotion } = live.current
      const { level, speaking = false, working = false, connecting = false } = sample()
      const t = reduceMotion ? 0 : now / 1000
      const accent = dark ? '#ffffff' : '#000000'
      const tint = speaking ? accent : (color ?? accent)
      const target = connecting ? 1 : 0
      mix = reduceMotion || !last ? target : mix + (target - mix) * Math.min(1, ((now - last) / 1000) * 8)
      last = now
      ctx.setTransform(scale, 0, 0, scale, 0, 0)
      ctx.clearRect(0, 0, width, height)
      ctx.globalAlpha = mix
      if (mix > 0.01) drawConnecting(ctx, width, height, t, accent, reduceMotion)
      ctx.globalAlpha = 1 - mix
      const count = mix < 0.99 ? Math.max(1, Math.floor(width / SPACING)) : 0
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
      ctx.globalAlpha = 1
      frame = requestAnimationFrame(draw)
    }
    frame = requestAnimationFrame(draw)
    return () => cancelAnimationFrame(frame)
  }, [scale, width, height])

  return <canvas ref={canvas} role="img" aria-label={label} className={className} style={{ width, height }} />
}

/** The connecting line: a faint track and a segment a third its length gliding back and forth (`t` in seconds), resting at the left with reduced motion. */
function drawConnecting(ctx: CanvasRenderingContext2D, width: number, height: number, t: number, accent: string, reduceMotion: boolean) {
  ctx.fillStyle = withAlpha(accent, 0.15)
  ctx.beginPath()
  ctx.roundRect(0, height / 2 - 0.75, width, 1.5, 0.75)
  ctx.fill()
  const segment = width / 3
  // A cosine turns each end smoothly instead of jumping back to the start.
  const progress = reduceMotion ? 0 : (1 - Math.cos((2 * Math.PI * t) / PERIOD)) / 2
  ctx.fillStyle = accent
  ctx.beginPath()
  ctx.roundRect(progress * (width - segment), height / 2 - 1.75, segment, 3.5, 1.75)
  ctx.fill()
}
