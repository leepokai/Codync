// Pure edits the canvas applies to a graph. Saving and undo stay with the view.

import { SNAP, snap } from './camera.ts'
import { connectionProblem, edgeId, makeNode, newId, nodeBox, type Branch, type Graph, type Node, type NodeKind, type Point, type Stroke } from './workflow.ts'

/** Room above a card for its kind label, so the label does not sit on the card above. */
const LABEL_CLEARANCE = 28

export function replaceNode(graph: Graph, id: string, update: (node: Node) => Node): Graph {
  return { ...graph, nodes: graph.nodes.map((node) => (node.id === id ? update(node) : node)) }
}

/** Move unlocked nodes by a world delta and land on the 8-point grid. */
export function moveNodes(graph: Graph, origins: Map<string, Point>, dx: number, dy: number): Graph {
  return {
    ...graph,
    nodes: graph.nodes.map((node) => {
      const origin = origins.get(node.id)
      if (!origin || node.locked) return node
      return { ...node, at: { x: snap(origin.x + dx), y: snap(origin.y + dy) } }
    }),
  }
}

export function nudgeNodes(graph: Graph, ids: Set<string>, dx: number, dy: number): Graph {
  return {
    ...graph,
    nodes: graph.nodes.map((node) => (ids.has(node.id) && !node.locked ? { ...node, at: { x: snap(node.at.x + dx), y: snap(node.at.y + dy) } } : node)),
  }
}

export function deleteSelection(graph: Graph, nodeIds: Set<string>, edgeIds: Set<string>): Graph {
  const drop = new Set([...nodeIds].filter((id) => !graph.nodes.find((node) => node.id === id)?.locked))
  return {
    nodes: graph.nodes.filter((node) => !drop.has(node.id)),
    edges: graph.edges.filter((edge) => !edgeIds.has(edge.id) && !drop.has(edge.from) && !drop.has(edge.to)),
    strokes: graph.strokes,
  }
}

export function addConnection(graph: Graph, from: string, to: string, branch?: Branch | null): { graph: Graph; error: string | null } {
  const error = connectionProblem(graph, from, to, branch)
  if (error) return { graph, error }
  return { graph: { ...graph, edges: [...graph.edges, { id: edgeId(from, to, branch), from, to, type: 'text', branch: branch ?? null }] }, error: null }
}

/**
 * Push a card down when the one above it, in the same column, grew into it. Measured heights
 * come from the rendered cards. Locked cards stay put and still push what is below them.
 */
export function spreadOverlaps(graph: Graph, heights: Map<string, number>): Graph | null {
  const known = graph.nodes.filter((node) => heights.has(node.id))
  if (known.length < 2) return null
  const ys = new Map(graph.nodes.map((node) => [node.id, node.at.y]))
  const ordered = [...known].sort((a, b) => a.at.y - b.at.y || a.at.x - b.at.x)
  let changed = false
  for (let i = 0; i < ordered.length; i++) {
    const below = ordered[i]
    if (below.locked) continue
    let y = ys.get(below.id) ?? below.at.y
    const box = nodeBox(below, heights.get(below.id))
    for (let j = 0; j < i; j++) {
      const above = ordered[j]
      const aboveY = ys.get(above.id) ?? above.at.y
      const aboveBox = nodeBox(above, heights.get(above.id))
      if (aboveBox.x + aboveBox.width <= box.x + 8 || box.x + box.width <= aboveBox.x + 8) continue
      const floor = aboveY + aboveBox.height + LABEL_CLEARANCE
      if (y < floor) {
        y = floor
        changed = true
      }
    }
    ys.set(below.id, y)
  }
  if (!changed) return null
  return {
    ...graph,
    nodes: graph.nodes.map((node) => {
      const y = ys.get(node.id)
      if (y === undefined || y === node.at.y) return node
      return { ...node, at: { x: node.at.x, y: Math.ceil(y / SNAP) * SNAP } }
    }),
  }
}

export function addNode(graph: Graph, kind: NodeKind, at: Point, link?: { from: string; branch?: Branch | null }): Graph {
  const node = makeNode(kind, { x: snap(at.x), y: snap(at.y) })
  let next: Graph = { ...graph, nodes: [...graph.nodes, node] }
  if (!link) return next
  const connected = addConnection(next, link.from, node.id, link.branch)
  return connected.error ? next : connected.graph
}

export function duplicateNodes(graph: Graph, ids: Set<string>, offset = SNAP * 2): { graph: Graph; ids: string[] } {
  const map = new Map<string, string>()
  const copies = graph.nodes.filter((node) => ids.has(node.id)).map((node) => {
    const id = newId()
    map.set(node.id, id)
    return { ...structuredClone(node), id, at: { x: snap(node.at.x + offset), y: snap(node.at.y + offset) } }
  })
  const edges = graph.edges.flatMap((edge) => {
    const from = map.get(edge.from)
    const to = map.get(edge.to)
    if (!from || !to) return []
    return [{ ...edge, id: edgeId(from, to, edge.branch, edge.id.endsWith(':order')), from, to }]
  })
  return { graph: { ...graph, nodes: [...graph.nodes, ...copies], edges: [...graph.edges, ...edges] }, ids: [...map.values()] }
}

export function addStroke(graph: Graph, stroke: Stroke): Graph {
  return { ...graph, strokes: [...graph.strokes, stroke] }
}

/** Drop drawings whose points fall inside the eraser, in world units. */
export function eraseStrokes(graph: Graph, at: Point, radius: number): Graph {
  const strokes = graph.strokes.filter((stroke) => !stroke.points.some((point) => Math.hypot(point.x - at.x, point.y - at.y) <= radius))
  return strokes.length === graph.strokes.length ? graph : { ...graph, strokes }
}
