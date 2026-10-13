import { useEffect, useRef } from 'react'
import { useDark, useReduceMotion } from '../lib/theme'

// thinking-orbs 0.3.1, commit de85557 (MIT, Jakub Antalik): src/engine/{core,orbits,lattice,web,morph}.ts
// with the resolved 20/64 pt presets, as ported in kit's ThinkingOrbGeometry.

export type OrbState = 'working' | 'searching' | 'listening' | 'connecting' | 'shaping'

interface Dot { x: number; y: number; z: number; radius: number; white: number; alpha: number }
interface Line { x1: number; y1: number; x2: number; y2: number; white: number; alpha: number; width: number }
type P3 = [number, number, number]
type P2 = [number, number]

function speed(state: OrbState, size: number) {
  const small = size < 40
  switch (state) {
    case 'working': return small ? 3.9 : 1.885
    case 'searching': return small ? 2.665 : 2.015
    case 'listening': return small ? 3.998 : 4.388
    case 'connecting': return small ? 6.63 : 3.315
    case 'shaping': return small ? 2.08 : 2.405
  }
}

function projector(yaw: number, tilt: number, size: number, scale: number) {
  const sy = Math.sin(yaw), cy = Math.cos(yaw), st = Math.sin(tilt), ct = Math.cos(tilt)
  return (p: P3): P3 => {
    const x = p[0] * cy + p[2] * sy
    const z = -p[0] * sy + p[2] * cy
    return [size / 2 + x * scale, size / 2 - (p[1] * ct - z * st) * scale, p[1] * st + z * ct]
  }
}
const hash = (a: number, b: number) => {
  const h = Math.sin(a * 12.9898 + b * 78.233) * 43758.5453
  return h - Math.floor(h)
}
function noise(x: number, y: number) {
  const xi = Math.floor(x), yi = Math.floor(y)
  const dx = x - xi, dy = y - yi
  const fx = dx * dx * (3 - 2 * dx), fy = dy * dy * (3 - 2 * dy)
  const a = hash(xi, yi), b = hash(xi + 1, yi), c = hash(xi, yi + 1), d = hash(xi + 1, yi + 1)
  return a + (b - a) * fx + (c - a) * fy + (a - b - c + d) * fx * fy
}
const len = (p: P3) => Math.hypot(p[0], p[1], p[2])
const scale3 = (p: P3, s: number): P3 => [p[0] * s, p[1] * s, p[2] * s]
const add3 = (a: P3, b: P3): P3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
const sub3 = (a: P3, b: P3): P3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]]

function orbits(size: number, t: number) {
  const small = size < 40, radius = (size / 2) * 0.82
  const project = projector(t * 0.12, 0.3, size, 1)
  const rs = Math.pow(size / 300, 0.6), multiplier = small ? 2.4 : 1
  const orbitCount = small ? 3 : 12, ghostCount = small ? 10 : 40
  const dots: Dot[] = []
  for (let orb = 0; orb < orbitCount; orb++) {
    const h1 = hash(orb, 1.7), h2 = hash(orb, 5.2), h3 = hash(orb, 8.9)
    const ro = radius * (0.45 + 0.52 * h1), theta = h1 * 2 * Math.PI, phi = Math.acos(2 * h2 - 1)
    const normal: P3 = [Math.sin(phi) * Math.cos(theta), Math.cos(phi), Math.sin(phi) * Math.sin(theta)]
    let u: P3 = [-normal[1], normal[0], 0]
    u = scale3(u, 1 / Math.max(1e-6, len(u)))
    const v: P3 = [-normal[2] * u[1], normal[2] * u[0], normal[0] * u[1] - normal[1] * u[0]]
    const sp = (0.25 + 0.55 * h3) * (h3 > 0.5 ? 1 : -1)
    for (let k = 0; k < ghostCount; k++) {
      const angle = (k / ghostCount) * 2 * Math.PI
      const p = project(scale3(add3(scale3(u, Math.cos(angle)), scale3(v, Math.sin(angle))), ro))
      const depth = (p[2] / ro + 1) / 2
      dots.push({ x: p[0], y: p[1], z: p[2], radius: 0.9 * multiplier * rs, white: 0.72, alpha: 0.5 * (0.4 + 0.6 * depth) })
    }
    for (let m = 0; m < 3; m++) {
      const angle = t * sp + (m / 3) * 2 * Math.PI + h2 * 6
      const p = project(scale3(add3(scale3(u, Math.cos(angle)), scale3(v, Math.sin(angle))), ro))
      const depth = (p[2] / ro + 1) / 2
      dots.push({ x: p[0], y: p[1], z: p[2], radius: (1.2 + 1.6 * depth) * multiplier * rs, white: 0.3 - 0.22 * depth, alpha: 1 })
    }
  }
  return { dots, lines: [] as Line[] }
}

function lattice(size: number, t: number, listening: boolean) {
  const small = size < 40, rs = Math.pow(size / 300, 0.6)
  const rings = listening ? (small ? 5 : 9) : small ? 6 : 11
  const density = listening ? (small ? 13 : 23) : small ? 14 : 29
  const multiplier = listening ? (small ? 1.6 : 1) : small ? 1.75 : 1.15
  const radius = (size / 2) * (listening ? 0.874 : 0.82)
  const tilt = listening ? 0.38 : 0.4 + 0.06 * Math.sin(t * 0.35)
  const project = projector(t * (listening ? 0.18 : 0.5), tilt, size, listening ? 1 : radius)
  const scan = t * (0.5 + 1.2 * (small ? 4.335 : 4.08))
  const dots: Dot[] = []
  for (let ri = 0; ri <= rings; ri++) {
    const lat = -Math.PI / 2 + (ri / rings) * Math.PI
    const cosLat = Math.cos(lat), sinLat = Math.sin(lat)
    const wave = 0.62 * Math.sin(t * 2.1 - ri * 0.52) + 0.38 * Math.sin(t * 1.27 + ri * 0.83)
    const rr = listening ? radius * (0.88 + 0.105 * wave) : 1
    const lonCount = Math.max(1, Math.round(Math.abs(cosLat) * density))
    for (let j = 0; j < lonCount; j++) {
      const lon = (j / lonCount) * 2 * Math.PI
      const p = project(scale3([cosLat * Math.cos(lon), sinLat, cosLat * Math.sin(lon)], rr))
      const depth = (p[2] / (listening ? radius : 1) + 1) / 2
      if (listening) {
        const crest = Math.max(0, wave)
        dots.push({ x: p[0], y: p[1], z: p[2], radius: (0.6 + 1.7 * depth) * multiplier * (1 + 0.4 * crest) * rs, white: 0.66 - 0.56 * depth - 0.1 * crest, alpha: 1 })
      } else {
        const angle = lon + t * 0.5 - scan
        const delta = Math.atan2(Math.sin(angle), Math.cos(angle))
        const boost = Math.exp(-(delta * delta) / 0.18) * Math.max(0, p[2])
        dots.push({ x: p[0], y: p[1], z: p[2], radius: ((0.6 + 1.7 * depth) * multiplier + boost) * rs, white: 0.62 - 0.54 * depth, alpha: 0.45 + 0.55 * Math.min(1, boost) })
      }
    }
  }
  return { dots, lines: [] as Line[] }
}

function web(size: number, t: number) {
  const small = size < 40, rs = Math.pow(size / 300, 0.6)
  const count = small ? 8 : 41, signals = small ? 1 : 7
  const multiplier = small ? 1.52 : 0.95, threshold = 0.72
  const project = projector(t * 0.12, 0.32, size, (size / 2) * 0.8)
  const golden = Math.PI * (3 - Math.sqrt(5))
  const nodes: P3[] = []
  for (let i = 0; i < count; i++) {
    const y = 1 - (2 * (i + 0.5)) / count
    const r = Math.sqrt(1 - y * y), a = i * golden
    const p: P3 = [r * Math.cos(a), y, r * Math.sin(a)]
    p[0] += 0.3 * (noise(i * 0.31 + 9, t * 0.24) - 0.5) * 2
    p[1] += 0.3 * (noise(i * 0.53 + 27, t * 0.21) - 0.5) * 2
    p[2] += 0.3 * (noise(i * 0.77 + 55, t * 0.27) - 0.5) * 2
    nodes.push(scale3(p, 1 / len(p)))
  }
  const dots: Dot[] = []
  const lines: Line[] = []
  for (let i = 0; i < count; i++) {
    for (let j = i + 1; j < count; j++) {
      const distance = len(sub3(nodes[i]!, nodes[j]!))
      if (distance >= threshold) continue
      const a = project(nodes[i]!), b = project(nodes[j]!), depth = ((a[2] + b[2]) / 2 + 1) / 2
      lines.push({ x1: a[0], y1: a[1], x2: b[0], y2: b[1], white: 0.42, alpha: (1 - distance / threshold) * (0.3 + 0.55 * depth), width: Math.max(0.6, 0.8 * rs) })
    }
    const p = project(nodes[i]!), depth = (p[2] + 1) / 2
    const pulse = 1 + 0.25 * Math.sin(t * 1.4 + i * 2.7)
    dots.push({ x: p[0], y: p[1], z: p[2], radius: (1.4 + 1.8 * depth) * multiplier * pulse * rs, white: 0.55 - 0.45 * depth, alpha: 1 })
  }
  for (let s = 0; s < signals; s++) {
    const segment = Math.floor(t * 0.55 + s * 7.31)
    const a = Math.floor(hash(segment, s * 3.1 + 1.7) * count)
    const b = Math.floor(hash(segment, s * 5.7 + 4.2) * count)
    if (a === b) continue
    const tick = t * 0.55 + s * 7.31, fraction = tick - Math.floor(tick)
    const node = add3(nodes[a]!, scale3(sub3(nodes[b]!, nodes[a]!), fraction))
    const p = project(scale3(node, 1 / Math.max(1e-6, len(node)))), depth = (p[2] + 1) / 2
    dots.push({ x: p[0], y: p[1], z: p[2], radius: (1.4 * 1.5 + 1.8 * depth) * multiplier * rs, white: 0.05, alpha: 0.5 + 0.5 * depth })
  }
  return { dots, lines }
}

/** The closed outline through `vertices`, parameterized by arc length (0...1). */
function polygon(vertices: P2[]) {
  const lengths = vertices.map((a, i) => {
    const b = vertices[(i + 1) % vertices.length]!
    return Math.hypot(b[0] - a[0], b[1] - a[1])
  })
  const total = lengths.reduce((sum, l) => sum + l, 0)
  return (f: number): P2 => {
    let target = f * total, i = 0
    while (target > lengths[i]! && i < vertices.length - 1) target -= lengths[i++]!
    const a = vertices[i]!, b = vertices[(i + 1) % vertices.length]!, k = lengths[i] ? Math.min(1, target / lengths[i]!) : 0
    return [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k]
  }
}
const SHAPES = [
  (f: number): P2 => [Math.cos(-Math.PI / 2 + f * 2 * Math.PI) * 0.24, Math.sin(-Math.PI / 2 + f * 2 * Math.PI) * 0.24],
  polygon([[0, -0.26], [0.24, 0.16], [-0.24, 0.16]]),
  polygon([[0, -0.2], [0.2, -0.2], [0.2, 0.2], [-0.2, 0.2], [-0.2, -0.2]]),
]

/** A dotted outline that holds a circle, triangle and square in turn, morphing between them. */
function morph(size: number, t: number) {
  const small = size < 40, spread = 1.45, hold = 1.4, change = 0.9, segment = hold + change
  const cycle = t % (segment * SHAPES.length)
  const k = Math.min(SHAPES.length - 1, Math.floor(cycle / segment)), local = cycle - k * segment
  const x = (local - hold) / change, m = local > hold ? x * x * (3 - 2 * x) : 0
  const from = SHAPES[k]!, to = SHAPES[(k + 1) % SHAPES.length]!
  const samples = 160
  const points: P2[] = []
  for (let i = 0; i < samples; i++) {
    const a = from(i / samples), b = to(i / samples)
    points.push([(a[0] + (b[0] - a[0]) * m) * spread, (a[1] + (b[1] - a[1]) * m) * spread])
  }
  const lengths = points.map((a, i) => {
    const b = points[(i + 1) % samples]!
    return Math.hypot(b[0] - a[0], b[1] - a[1])
  })
  const total = lengths.reduce((sum, l) => sum + l, 0)
  const count = small ? 18 : 24
  const radius = Math.max(0.35, (small ? 0.021231 : 0.008295) * 1.35 * spread * size)
  const pulse = 1 + 0.02 * Math.sin(local * 3.1)
  const dots: Dot[] = []
  let i = 0, walked = 0
  for (let n = 0; n < count; n++) {
    const target = (n / count) * total
    while (walked + lengths[i]! < target && i < samples - 1) walked += lengths[i++]!
    const a = points[i]!, b = points[(i + 1) % samples]!, f = lengths[i] ? Math.min(1, (target - walked) / lengths[i]!) : 0
    dots.push({ x: size / 2 + (a[0] + (b[0] - a[0]) * f) * pulse * size, y: size / 2 + (a[1] + (b[1] - a[1]) * f) * pulse * size, z: 0, radius, white: 0.1, alpha: 1 })
  }
  return { dots, lines: [] as Line[] }
}

export function orbFrame(state: OrbState, size: number, time: number) {
  const f = state === 'working' ? orbits(size, time) : state === 'connecting' ? web(size, time) : state === 'shaping' ? morph(size, time) : lattice(size, time, state === 'listening')
  return {
    dots: f.dots.filter((d) => d.alpha >= 0.02).map((d) => ({ ...d, radius: Math.max(0.3, d.radius) })).sort((a, b) => a.z - b.z),
    lines: f.lines.filter((l) => l.alpha >= 0.02),
  }
}

const ink = (white: number, alpha: number) => (1 - Math.min(1, Math.max(0, white))) * alpha

/** Thinking-orbs states with their tuned dot geometry and depth shading (decorative). */
export function ThinkingOrb({ state = 'working', size = 16, color = 'var(--text)', animated = true }: { state?: OrbState; size?: number; color?: string; animated?: boolean }) {
  const canvas = useRef<HTMLCanvasElement>(null)
  const reduce = useReduceMotion()
  const dark = useDark()
  useEffect(() => {
    const el = canvas.current
    if (!el) return
    const ratio = window.devicePixelRatio || 1
    el.width = Math.round(size * ratio)
    el.height = Math.round(size * ratio)
    const ctx = el.getContext('2d')!
    const resolved = getComputedStyle(el).color
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
      const time = 0.6 + (running ? ((now - epoch) / 1000) * speed(state, size) : 0)
      const f = orbFrame(state, size, time)
      ctx.setTransform(ratio, 0, 0, ratio, 0, 0)
      ctx.clearRect(0, 0, size, size)
      ctx.strokeStyle = resolved
      ctx.fillStyle = resolved
      for (const l of f.lines) {
        ctx.globalAlpha = ink(l.white, l.alpha)
        ctx.lineWidth = l.width
        ctx.beginPath()
        ctx.moveTo(l.x1, l.y1)
        ctx.lineTo(l.x2, l.y2)
        ctx.stroke()
      }
      for (const d of f.dots) {
        ctx.globalAlpha = ink(d.white, d.alpha)
        ctx.beginPath()
        ctx.arc(d.x, d.y, d.radius, 0, Math.PI * 2)
        ctx.fill()
      }
      ctx.globalAlpha = 1
      if (running) frame = requestAnimationFrame(draw)
    }
    frame = requestAnimationFrame(draw)
    return () => cancelAnimationFrame(frame)
  }, [state, size, animated, reduce, color, dark])
  return <canvas ref={canvas} aria-hidden style={{ width: size, height: size, color, display: 'block', flex: 'none' }} />
}
