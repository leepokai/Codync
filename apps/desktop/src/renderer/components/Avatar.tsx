import { memo, useEffect, useRef, type CSSProperties } from 'react'
import { isGroup, isWorking, needsInput, type Bot } from '@shared/models'
import { DOT_RADIUS, dotGrid } from '../lib/dot-grid'
import { avatarColor, palette, useDark, usePixelRatio, useReduceMotion } from '../lib/theme'
import { Icon } from './Icon'
import './avatar.css'
import { LeadAgentBadge } from './LeadAgentBadge'

export type Mood = 'idle' | 'working' | 'needsInput' | 'failed'

/** The eight Grok Bot character silhouettes, in a w×h box. */
export function characterPath(kind: string, w: number, h: number) {
  const p = new Path2D()
  let stroke = 0
  switch (kind) {
    case 'pebble':
      p.ellipse(w / 2, h / 2, w / 2, h * 0.4, 0, 0, Math.PI * 2)
      break
    case 'squircle':
      p.roundRect(w * 0.04, h * 0.04, w * 0.92, h * 0.92, w * 0.3)
      break
    case 'tablet':
      p.roundRect(w * 0.14, 0, w * 0.72, h, w * 0.22)
      break
    case 'wedge':
      p.moveTo(w * 0.5, h * 0.04)
      p.quadraticCurveTo(w * 0.9, h * 0.4, w * 0.98, h * 0.86)
      p.quadraticCurveTo(w * 0.5, h * 1.04, w * 0.02, h * 0.86)
      p.quadraticCurveTo(w * 0.1, h * 0.4, w * 0.5, h * 0.04)
      break
    case 'hex':
      for (let i = 0; i < 6; i++) {
        const a = (i * Math.PI) / 3 - Math.PI / 2
        const x = w / 2 + Math.cos(a) * w * 0.49
        const y = h / 2 + Math.sin(a) * h * 0.49
        if (i === 0) p.moveTo(x, y)
        else p.lineTo(x, y)
      }
      p.closePath()
      stroke = w * 0.08
      break
    case 'cloud':
      p.ellipse(w * 0.275, h * 0.575, w * 0.275, h * 0.275, 0, 0, Math.PI * 2)
      p.moveTo(w, h * 0.575)
      p.ellipse(w * 0.725, h * 0.575, w * 0.275, h * 0.275, 0, 0, Math.PI * 2)
      p.moveTo(w * 0.82, h * 0.4)
      p.ellipse(w * 0.5, h * 0.4, w * 0.32, h * 0.32, 0, 0, Math.PI * 2)
      p.roundRect(w * 0.1, h * 0.5, w * 0.8, h * 0.35, w * 0.15)
      break
    case 'teardrop':
      p.moveTo(w * 0.5, 0)
      p.bezierCurveTo(w * 0.62, h * 0.2, w * 0.94, h * 0.38, w * 0.94, h * 0.62)
      p.arc(w * 0.5, h * 0.62, w * 0.44, 0, Math.PI, false)
      p.bezierCurveTo(w * 0.06, h * 0.38, w * 0.38, h * 0.2, w * 0.5, 0)
      break
    default: {
      // blob
      const steps = 64
      for (let i = 0; i <= steps; i++) {
        const a = (i / steps) * 2 * Math.PI
        const rr = 0.46 + 0.035 * Math.sin(a * 3 + 0.6)
        const x = w / 2 + Math.cos(a) * w * rr
        const y = h / 2 + Math.sin(a) * h * rr
        if (i === 0) p.moveTo(x, y)
        else p.lineTo(x, y)
      }
      p.closePath()
    }
  }
  return { path: p, stroke }
}

interface Dot {
  row: number
  col: number
  x: number
  y: number
}

const probe = document.createElement('canvas').getContext('2d')!
const gridCache = new Map<string, Dot[]>()

/** Square-grid dot centers that fall inside the silhouette. */
function grid(shape: string, size: number, cells: number): Dot[] {
  const key = `${shape}|${size}|${cells}`
  const hit = gridCache.get(key)
  if (hit) return hit
  const { path, stroke } = characterPath(shape, size, size)
  const step = size / cells
  const dots: Dot[] = []
  probe.lineWidth = stroke
  probe.lineJoin = 'round'
  for (let row = 0; row < cells; row++) {
    for (let col = 0; col < cells; col++) {
      const x = (col + 0.5) * step
      const y = (row + 0.5) * step
      if (probe.isPointInPath(path, x, y, 'nonzero') || (stroke > 0 && probe.isPointInStroke(path, x, y))) dots.push({ row, col, x, y })
    }
  }
  gridCache.set(key, dots)
  return dots
}

interface CharacterProps {
  shape: string
  /** An avatar palette id, or a CSS color with `tint`. */
  color: string
  tint?: string
  size?: number
  mood?: Mood
  /** Draws for a dark ground whatever the system appearance (the notch island is always black). */
  dark?: boolean
}

/**
 * Grok-Bot-style character drawn like the app icon: an even grid of dots shaded as if the
 * silhouette were a ball, with hollow eyes that glance while it works and blink now and then.
 */
export const CharacterAvatar = memo(function CharacterAvatar({ shape, color, tint, size = 40, mood = 'idle', dark: forceDark }: CharacterProps) {
  const canvas = useRef<HTMLCanvasElement>(null)
  const dark = useDark() || forceDark === true
  const reduce = useReduceMotion()
  const ratio = usePixelRatio()
  const fill = tint ?? avatarColor(color)
  // A whole number of device pixels, so the bitmap shows 1:1 and drawCharacter's pixels are the screen's.
  const pixels = Math.max(1, Math.round(size * ratio))

  useEffect(() => {
    const el = canvas.current
    if (!el) return
    el.width = pixels
    el.height = pixels
    const scale = pixels / size
    const ctx = el.getContext('2d')!
    const still = mood === 'idle' || mood === 'failed' || reduce
    // A moving character draws at 30 fps (its light turns slowly), and only while on screen.
    let frame = 0
    let drawn = -Infinity
    let visible = true
    const draw = (time: number) => {
      frame = 0
      if (time - drawn >= 33) {
        drawn = time
        ctx.setTransform(scale, 0, 0, scale, 0, 0)
        ctx.clearRect(0, 0, size, size)
        drawCharacter(ctx, { shape, fill, size, mood, dark, t: still ? 0 : (time / 1000) % 3600, still })
      }
      if (!still && visible) frame = requestAnimationFrame(draw)
    }
    draw(performance.now())
    if (still) return
    const onScreen = new IntersectionObserver(([e]) => {
      visible = e!.isIntersecting
      if (visible && !frame) frame = requestAnimationFrame(draw)
    })
    onScreen.observe(el)
    return () => {
      onScreen.disconnect()
      cancelAnimationFrame(frame)
    }
  }, [shape, fill, size, mood, dark, reduce, pixels])

  return <canvas ref={canvas} aria-hidden style={{ width: pixels / ratio, height: pixels / ratio, display: 'block', flex: 'none' }} />
})

/** Draws one frame of a character at the context's origin (views and menu bar icons). */
export function drawCharacter(
  ctx: CanvasRenderingContext2D,
  { shape, fill, size, mood, dark, t, still }: { shape: string; fill: string; size: number; mood: Mood; dark: boolean; t: number; still: boolean },
) {
  // The grid fits the context's device pixels (its scale).
  const { cells, pitch, first } = dotGrid(size, ctx.getTransform().a || 1)
  const eyeColumns = cells === 7 ? [2, 4] : cells === 9 ? [3, 6] : [4, 8]
  const eyeRowsAll = cells === 13 ? [4, 5, 6] : cells === 9 ? [3, 4] : [2, 3]
  const dots = grid(shape, size, cells)
  const [tr, tg, tb] = palette(dark).text
  const n = parseInt(fill.slice(1), 16)
  const [cr, cg, cb] = [(n >> 16) & 255, (n >> 8) & 255, n & 255]
  // Glance: whole-cell steps left / center / right, like a small display.
  const glance = mood === 'working' ? Math.round(Math.sin((t * 2 * Math.PI) / 3.2) * 1.4) : 0
  const blinking = !still && (t / 4.7) % 1 < 0.035
  const eyeRows = blinking ? eyeRowsAll.slice(-1) : eyeRowsAll
  const eyeCols = eyeColumns.map((c) => c + glance)
  const half = size / 2
  const yaw = mood === 'working' ? t * 1.4 : -0.7
  const lx = Math.sin(yaw) * 0.8
  const ly = 0.55
  const lz = Math.cos(yaw) * 0.5 + 0.6 // never fully behind
  const ll = Math.hypot(lx, ly, lz)
  const mouthCenter = Math.floor(cells / 2)
  const closedEyeRow = eyeRowsAll[eyeRowsAll.length - 1]!
  for (const d of dots) {
    // Closed eyes droop away from the nose, like a disappointed face.
    const isEye = mood === 'failed'
      ? eyeColumns.some((col) => Math.abs(d.col - col) <= 1
        && d.row === closedEyeRow - (Math.abs(d.col - mouthCenter) < Math.abs(col - mouthCenter) ? 1 : 0))
      : eyeCols.includes(d.col) && eyeRows.includes(d.row)
    if (isEye) continue
    // A downturned mouth, cut out of the same dot grid as the eyes.
    const mouthOffset = Math.abs(d.col - mouthCenter)
    if (mood === 'failed' && mouthOffset <= 1 && d.row === Math.floor(cells * 0.65) + mouthOffset) continue
    const u = (d.x - half) / half
    const v = (half - d.y) / half
    const z = Math.sqrt(Math.max(0.2, 1 - u * u - v * v))
    const nl = Math.hypot(u, v, z)
    let shade = 0.3 + 0.7 * Math.max(0, (u * lx + v * ly + z * lz) / (nl * ll))
    if (mood === 'needsInput') {
      const ripple = 0.5 + 0.5 * Math.sin(Math.hypot(u, v) * 9 - t * 5)
      shade *= 0.6 + 0.4 * ripple
    }
    const r = pitch * DOT_RADIUS * (0.55 + 0.45 * shade)
    const ink = cells < 13 ? 0.4 + 0.4 * shade : 0.2 + 0.4 * Math.min(1, shade / 0.7)
    ctx.beginPath()
    ctx.arc(first + d.col * pitch, first + d.row * pitch, r, 0, Math.PI * 2)
    ctx.fillStyle = `rgba(${tr},${tg},${tb},${ink})`
    ctx.fill()
    if (shade > 0.6) {
      ctx.fillStyle = `rgba(${cr},${cg},${cb},${(shade - 0.6) / 0.4})`
      ctx.fill()
    }
  }
}

export function moodOf(bot: Bot, animated = true): Mood {
  if (!animated) return 'idle'
  return needsInput(bot) ? 'needsInput' : isWorking(bot) ? 'working' : 'idle'
}

export function BotAvatar({ bot, size = 40, animated = true }: { bot: Bot; size?: number; animated?: boolean }) {
  return <CharacterAvatar shape={bot.avatarShape} color={bot.avatarColor} size={size} mood={moodOf(bot, animated)} />
}

/**
 * A group chat's face, clustered like iMessage: two bots tucked diagonally, three in a
 * triangle, four in a 2×2 grid; past four the last cell counts the rest.
 */
export function GroupAvatar({ members, size = 40, animated = true }: { members: Bot[]; size?: number; animated?: boolean }) {
  const place = (bot: Bot, side: number, corner: CSSProperties) => (
    <div key={bot.id} className="avatar-slot" style={corner}>
      <BotAvatar bot={bot} size={side} animated={animated} />
    </div>
  )
  let content
  switch (members.length) {
    case 0:
      content = (
        <div className="avatar-empty">
          <Icon name="person.2" size={size * 0.4} />
        </div>
      )
      break
    case 1:
      content = <BotAvatar bot={members[0]!} size={size} animated={animated} />
      break
    case 2:
      content = [place(members[1]!, size * 0.66, { top: 0, right: 0 }), place(members[0]!, size * 0.66, { bottom: 0, left: 0 })]
      break
    case 3:
      content = [
        place(members[0]!, size * 0.55, { top: 0, left: (size - size * 0.55) / 2 }),
        place(members[1]!, size * 0.55, { bottom: 0, left: 0 }),
        place(members[2]!, size * 0.55, { bottom: 0, right: 0 }),
      ]
      break
    default:
      content = [
        place(members[0]!, size * 0.5, { top: 0, left: 0 }),
        place(members[1]!, size * 0.5, { top: 0, right: 0 }),
        place(members[2]!, size * 0.5, { bottom: 0, left: 0 }),
        members.length === 4 ? (
          place(members[3]!, size * 0.5, { bottom: 0, right: 0 })
        ) : (
          <div key="more" className="avatar-more" style={{ width: size * 0.5, height: size * 0.5, fontSize: size * 0.24 }}>
            +{members.length - 3}
          </div>
        ),
      ]
  }
  return <div aria-hidden className="avatar-frame" style={{ width: size, height: size }}>{content}</div>
}

/** Avatar with the roster status dot: filled = unread, amber = needs you. A group shows its members. */
export function AvatarWithStatus({ bot, members = [], size = 44 }: { bot: Bot; members?: Bot[]; size?: number }) {
  return (
    <div className={`avatar-frame ${bot.lead ? 'lead-avatar' : ''}`} style={{ width: size, height: size }}>
      {isGroup(bot) ? <GroupAvatar members={members} size={size} /> : <BotAvatar bot={bot} size={size} />}
      {bot.lead ? <LeadAgentBadge size={Math.max(12, size * 0.34)} /> : null}
      {needsInput(bot) ? (
        <div className="avatar-badge needs-you" style={{ width: size * 0.36, height: size * 0.36 }}>
          <Icon name="exclamationmark" size={size * 0.2} weight="black" scaled={false} />
        </div>
      ) : bot.unread > 0 ? (
        <div className="avatar-badge unread" style={{ width: size * 0.28, height: size * 0.28 }} />
      ) : null}
    </div>
  )
}
