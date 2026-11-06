// Routine flows (host/src/routines/flow/, docs/features/routine-flows.md). The host owns the rules:
// it validates every change and runs the flow. This file only shapes what the canvas shows: node
// positions (saved, else automatic columns), edges, the list that mirrors the canvas, and a
// pre-check of the connect rules so a drop that the host would refuse never leaves the app.

import type { Routine, RoutineRun } from '../views/routines/routine'

export type StepStatus = 'running' | 'succeeded' | 'failed' | 'skipped' | 'cancelled'

export interface FlowLink {
  id: string
  routineId: string
  fromBotId: string
  toBotId: string
  instruction: string
  createdAt: number
}

export interface FlowStep {
  id: string
  runId: string
  botId: string
  deliveryId: string | null
  input: string
  output: string | null
  status: StepStatus
  error: string | null
  createdAt: number
  updatedAt: number
}

export interface FlowPosition {
  nodeKey: string
  x: number
  y: number
}

export interface FlowRoutineCard {
  routine: Routine
  /** Newest first. */
  recentRuns: RoutineRun[]
  upcomingRuns: number[]
  /** The steps of the newest run that has any, oldest first. */
  steps: FlowStep[]
}

export interface FlowCanvas {
  botId: string
  rev: number
  routines: FlowRoutineCard[]
  links: FlowLink[]
  positions: FlowPosition[]
  placedBotIds: string[]
}

/** The host's limit on how far a flow reaches from its routine's bot. */
export const DEFAULT_MAX_DEPTH = 5
export const MAX_COORDINATE = 1_000_000
/** The size a node card takes on the canvas; the columns and the edge anchors follow it. */
export const NODE_WIDTH = 220
export const NODE_HEIGHT = 96
const COLUMN_GAP = 90
const ROW_GAP = 40

export const routineKey = (id: string) => `routine:${id}`
export const botKey = (id: string) => `bot:${id}`

export interface FlowNode {
  key: string
  kind: 'routine' | 'bot'
  /** The routine's or the bot's id. */
  id: string
  x: number
  y: number
  /** Whether the position was saved on the host (else it is the automatic column). */
  saved: boolean
}

export interface FlowEdge {
  id: string
  from: string
  to: string
  /** The link this edge is; null for a routine's fixed edge to its own bot. */
  linkId: string | null
}

export const clamp = (value: number, low: number, high: number) => Math.min(high, Math.max(low, value))
export const clampCoordinate = (value: number) => clamp(Math.round(value), -MAX_COORDINATE, MAX_COORDINATE)

/** The bot that starts a routine's flow. */
export function ownerOf(canvas: FlowCanvas, routineId: string): string | null {
  return canvas.routines.find((card) => card.routine.id === routineId)?.routine.botId ?? null
}

/** How many hops from the routine's own bot each bot of its flow is (the longest path); absent when not reached. */
export function depths(owner: string, links: readonly FlowLink[]): Map<string, number> {
  const depth = new Map<string, number>([[owner, 0]])
  // The host refuses cycles, so a depth only grows, at most once per link; the cap keeps a bad canvas from spinning.
  for (let round = 0; round <= links.length; round++) {
    let moved = false
    for (const link of links) {
      const from = depth.get(link.fromBotId)
      if (from === undefined || (depth.get(link.toBotId) ?? -1) >= from + 1) continue
      depth.set(link.toBotId, from + 1)
      moved = true
    }
    if (!moved) break
  }
  return depth
}

function reaches(links: readonly FlowLink[], start: string, target: string): boolean {
  const seen = new Set<string>()
  const queue = [start]
  for (let next = queue.pop(); next !== undefined; next = queue.pop()) {
    if (next === target) return true
    if (seen.has(next)) continue
    seen.add(next)
    for (const link of links) if (link.fromBotId === next) queue.push(link.toBotId)
  }
  return false
}

function longestFrom(links: readonly FlowLink[], bot: string, seen = new Set<string>()): number {
  if (seen.has(bot)) return 0
  seen.add(bot)
  let best = 0
  for (const link of links) if (link.fromBotId === bot) best = Math.max(best, 1 + longestFrom(links, link.toBotId, seen))
  seen.delete(bot)
  return best
}

/** Why `from` can't hand a routine's reply to `to` (the host's rules, in its words), or null when it can. */
export function connectProblem(canvas: FlowCanvas, routineId: string, from: string, to: string, maxDepth = DEFAULT_MAX_DEPTH): string | null {
  const owner = ownerOf(canvas, routineId)
  if (!owner) return 'That routine is not on this canvas.'
  const links = canvas.links.filter((link) => link.routineId === routineId)
  if (from === to) return "A bot can't hand work to itself."
  if (to === owner) return "The routine's own bot starts the flow, so nothing can hand work back to it."
  const reached = depths(owner, links)
  const here = reached.get(from)
  if (here === undefined) return "The flow doesn't reach that bot yet: connect it from the routine's bot first."
  if (links.some((link) => link.fromBotId === from && link.toBotId === to)) return 'These two bots are already connected in this routine.'
  if (reaches(links, to, from)) return 'That link would make the flow loop.'
  if (here + 1 + longestFrom(links, to) > maxDepth) return `A flow reaches at most ${maxDepth} bots deep.`
  return null
}

/** The bots a canvas shows: the canvas's own, every routine's bot, every link's ends and the bots placed by hand. */
export function canvasBotIds(canvas: FlowCanvas): string[] {
  const ids = new Set<string>([canvas.botId])
  for (const card of canvas.routines) ids.add(card.routine.botId)
  for (const link of canvas.links) {
    ids.add(link.fromBotId)
    ids.add(link.toBotId)
  }
  for (const id of canvas.placedBotIds) ids.add(id)
  return [...ids]
}

/**
 * Every node with its place: the saved one, else an automatic column. Routines stand in the first
 * column; a bot goes one column right of the deepest routine flow that reaches it, bots no flow
 * reaches (placed by hand without a link) in the last column. A node never moved keeps its column
 * as the canvas grows, so the layout is stable.
 */
export function layoutNodes(canvas: FlowCanvas): FlowNode[] {
  const saved = new Map(canvas.positions.map((p) => [p.nodeKey, p]))
  const column = new Map<string, number>()
  for (const card of canvas.routines) {
    const reached = depths(card.routine.botId, canvas.links.filter((l) => l.routineId === card.routine.id))
    for (const [bot, depth] of reached) column.set(bot, Math.max(column.get(bot) ?? 0, depth + 1))
  }
  const lastColumn = Math.max(1, ...column.values()) + 1
  const rows = new Map<number, number>()
  const place = (key: string, kind: FlowNode['kind'], id: string, col: number): FlowNode => {
    const position = saved.get(key)
    const row = rows.get(col) ?? 0
    rows.set(col, row + 1)
    const x = position?.x ?? col * (NODE_WIDTH + COLUMN_GAP)
    const y = position?.y ?? row * (NODE_HEIGHT + ROW_GAP)
    return { key, kind, id, x, y, saved: position !== undefined }
  }
  const nodes = canvas.routines.map((card) => place(routineKey(card.routine.id), 'routine', card.routine.id, 0))
  for (const id of canvasBotIds(canvas)) nodes.push(place(botKey(id), 'bot', id, column.get(id) ?? lastColumn))
  return nodes
}

/** Each routine's fixed edge to its bot, then the links. */
export function flowEdges(canvas: FlowCanvas): FlowEdge[] {
  const starts = canvas.routines.map((card) => ({ id: `start:${card.routine.id}`, from: routineKey(card.routine.id), to: botKey(card.routine.botId), linkId: null }))
  return [...starts, ...canvas.links.map((link) => ({ id: link.id, from: botKey(link.fromBotId), to: botKey(link.toBotId), linkId: link.id }))]
}

/** The latest step the canvas holds for a bot, across its routines. */
export function latestStep(canvas: FlowCanvas, botId: string): FlowStep | null {
  let best: FlowStep | null = null
  for (const card of canvas.routines) {
    for (const step of card.steps) {
      if (step.botId === botId && (!best || step.updatedAt >= best.updatedAt)) best = step
    }
  }
  return best
}

export function stepOf(card: FlowRoutineCard, botId: string): FlowStep | null {
  return card.steps.find((step) => step.botId === botId) ?? null
}

/** One line of the list that mirrors the canvas: a routine, or a bot under the bot that hands to it. */
export interface FlowRow {
  key: string
  routineId: string
  botId: string
  depth: number
  link: FlowLink | null
  step: FlowStep | null
}

const MAX_ROWS = 200

/** The canvas as a list, routine by routine and depth first: what the keyboard and a screen reader walk. */
export function flowRows(canvas: FlowCanvas): FlowRow[] {
  const rows: FlowRow[] = []
  for (const card of canvas.routines) {
    const links = canvas.links.filter((l) => l.routineId === card.routine.id)
    const walk = (botId: string, depth: number, link: FlowLink | null, path: readonly string[]) => {
      if (rows.length >= MAX_ROWS || path.includes(botId)) return
      rows.push({ key: `${card.routine.id}/${link?.id ?? 'start'}`, routineId: card.routine.id, botId, depth, link, step: stepOf(card, botId) })
      for (const next of links.filter((l) => l.fromBotId === botId)) walk(next.toBotId, depth + 1, next, [...path, botId])
    }
    walk(card.routine.botId, 0, null, [])
  }
  return rows
}

/** What a step's status says, in the words the other clients use. */
export function statusLabel(status: StepStatus | null): string {
  switch (status) {
    case 'running': return 'Running'
    case 'succeeded': return 'Succeeded'
    case 'failed': return 'Failed'
    case 'skipped': return 'Skipped'
    case 'cancelled': return 'Cancelled'
    default: return 'Not run yet'
  }
}

/** The canvas's empty state: nothing has run for it, or nothing is connected yet. */
export function isUnconnected(canvas: FlowCanvas, routineId: string): boolean {
  return !canvas.links.some((link) => link.routineId === routineId)
}

// MARK: view (pan and zoom)

export interface View {
  x: number
  y: number
  scale: number
}

export const MIN_SCALE = 0.3
export const MAX_SCALE = 2

/** Zoom by `factor` keeping the point (`px`, `py`) of the viewport still. */
export function zoomAt(view: View, factor: number, px: number, py: number): View {
  const scale = clamp(view.scale * factor, MIN_SCALE, MAX_SCALE)
  const k = scale / view.scale
  return { scale, x: px - (px - view.x) * k, y: py - (py - view.y) * k }
}

/** The view that shows every node inside a viewport of `width` × `height`, with a margin. */
export function fitView(nodes: readonly FlowNode[], width: number, height: number, margin = 40): View {
  if (nodes.length === 0 || width <= 0 || height <= 0) return { x: 0, y: 0, scale: 1 }
  const left = Math.min(...nodes.map((n) => n.x))
  const top = Math.min(...nodes.map((n) => n.y))
  const right = Math.max(...nodes.map((n) => n.x + NODE_WIDTH))
  const bottom = Math.max(...nodes.map((n) => n.y + NODE_HEIGHT))
  const scale = clamp(Math.min((width - 2 * margin) / (right - left), (height - 2 * margin) / (bottom - top)), MIN_SCALE, 1)
  return { scale, x: (width - (right - left) * scale) / 2 - left * scale, y: (height - (bottom - top) * scale) / 2 - top * scale }
}

/** Where an edge leaves a node (its right middle) and enters the next (its left middle). */
export function edgePath(from: FlowNode, to: FlowNode): string {
  const x1 = from.x + NODE_WIDTH
  const y1 = from.y + NODE_HEIGHT / 2
  const x2 = to.x
  const y2 = to.y + NODE_HEIGHT / 2
  const bend = Math.max(40, Math.abs(x2 - x1) / 2)
  return `M ${x1} ${y1} C ${x1 + bend} ${y1}, ${x2 - bend} ${y2}, ${x2} ${y2}`
}

/** The node whose card holds the canvas point, if any: where a dragged connection was dropped. */
export function nodeAt(nodes: readonly FlowNode[], x: number, y: number): FlowNode | null {
  return nodes.find((n) => x >= n.x && x <= n.x + NODE_WIDTH && y >= n.y && y <= n.y + NODE_HEIGHT) ?? null
}
