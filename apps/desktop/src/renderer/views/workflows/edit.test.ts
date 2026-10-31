import assert from 'node:assert/strict'
import { test } from 'node:test'
import { spreadOverlaps } from './edit.ts'
import { makeNode, type Graph } from './workflow.ts'

test('a card that grew into the one below is pushed down, leaving room for its label', () => {
  const upper = makeNode('agent', { x: 80, y: 80 })
  const lower = makeNode('agent', { x: 80, y: 280 })
  const graph: Graph = { nodes: [upper, lower], edges: [], strokes: [] }
  const next = spreadOverlaps(graph, new Map([[upper.id, 240], [lower.id, 240]]))
  assert.ok(next)
  assert.equal(next.nodes.find((node) => node.id === lower.id)?.at.y, 352)
  assert.equal(next.nodes.find((node) => node.id === upper.id)?.at.y, 80)
})

test('cards in different columns stay where they are', () => {
  const left = makeNode('agent', { x: 80, y: 80 })
  const right = makeNode('output', { x: 520, y: 80 })
  const graph: Graph = { nodes: [left, right], edges: [], strokes: [] }
  assert.equal(spreadOverlaps(graph, new Map([[left.id, 240], [right.id, 160]])), null)
})
