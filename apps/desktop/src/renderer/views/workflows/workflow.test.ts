import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  cloneGraphNewIds,
  connectionProblem,
  copyTitle,
  editMode,
  graphFromOutline,
  loopEdges,
  makeNode,
  fixedHeight,
  nodeBox,
  nodeTitle,
  splitJoinedInputs,
  outlineOf,
  ports,
  readingOrder,
  validate,
  type Graph,
  type Node,
} from './workflow.ts'

function node(id: string, kind: Node['kind'], fields: Node['fields'] = {}, at = { x: 0, y: 0 }): Node {
  return { id, kind, at, locked: false, fields, schedule: null }
}

function edge(from: string, to: string, branch?: 'yes' | 'no') {
  const id = branch ? `${from}.${branch}>${to}:text` : `${from}>${to}:text`
  return { id, from, to, type: 'text' as const, branch: branch ?? null }
}

function graph(nodes: Node[], edges: Graph['edges'] = []): Graph {
  return { nodes, edges, strokes: [] }
}

test('ports match the host: triggers and prompts only emit, output only accepts, notes have neither', () => {
  assert.deepEqual(ports('trigger'), [[], ['text']])
  assert.deepEqual(ports('prompt'), [[], ['text']])
  assert.deepEqual(ports('agent'), [['text'], ['text']])
  assert.deepEqual(ports('condition'), [['text'], ['text']])
  assert.deepEqual(ports('approval'), [['text'], ['text']])
  assert.deepEqual(ports('output'), [['text'], []])
  assert.deepEqual(ports('note'), [[], []])
  assert.deepEqual(ports('future'), [[], []])
})

test('a straight line round-trips through the outline', () => {
  const original = graph(
    [node('t', 'trigger', { trigger: 'manual' }), node('a', 'agent', { botId: 'bot', task: 'Summarize' }, { x: 400, y: 80 }), node('o', 'output', { target: 'notify' }, { x: 800, y: 80 })],
    [edge('t', 'a'), edge('a', 'o')],
  )
  const outline = outlineOf(original)
  assert.ok(outline)
  assert.deepEqual(outline.main, ['a', 'o'])
  assert.equal(editMode(original), 'outline')
  const again = outlineOf(graphFromOutline(original, outline))
  assert.deepEqual(again?.main, outline.main)
})

test('two conditions cannot be an outline', () => {
  const original = graph([
    node('t', 'trigger', { trigger: 'manual' }),
    node('c1', 'condition', { op: 'empty' }),
    node('c2', 'condition', { op: 'empty' }),
  ])
  assert.equal(outlineOf(original), null)
  assert.equal(editMode(original), 'fields')
})

test('a loop is allowed to connect and is named by loopEdges', () => {
  const original = graph(
    [node('a', 'agent'), node('b', 'agent', {}, { x: 40, y: 0 })],
    [edge('a', 'b')],
  )
  assert.equal(connectionProblem(original, 'b', 'a'), null)
  const looped = graph(original.nodes, [...original.edges, edge('b', 'a')])
  assert.equal(loopEdges(looped).size, 1)
  assert.equal(outlineOf(looped), null)
  assert.equal(connectionProblem(original, 'a', 'a'), "A step can't connect to itself")
  assert.equal(connectionProblem(looped, 'a', 'b'), 'Those steps are already connected')
})

test('reading order goes left to right, then top to bottom', () => {
  const original = graph(
    [
      node('t', 'trigger', { trigger: 'manual' }, { x: 0, y: 0 }),
      node('low', 'agent', {}, { x: 100, y: 40 }),
      node('high', 'prompt', { text: 'Hi' }, { x: 100, y: 0 }),
    ],
    [edge('t', 'high'), edge('t', 'low')],
  )
  assert.deepEqual(readingOrder(original), ['t', 'high', 'low'])
})

test('an unknown kind is read only and a bad line is refused', () => {
  assert.equal(editMode(graph([node('x', 'llm')])), 'readOnly')
  const self = graph([node('a', 'agent')], [edge('a', 'a')])
  assert.equal(validate(self)?.message, "A step can't connect to itself")
  const note = graph([node('n', 'note', { tone: 'yellow' }), node('a', 'agent')], [{ id: 'e', from: 'n', to: 'a', type: 'text' }])
  assert.equal(validate(note)?.kind, 'annotation')
})

test('titles and a copied workflow get new ids', () => {
  assert.equal(nodeTitle(node('a', 'agent', { botId: 'b' }), (id) => (id === 'b' ? 'Ada' : undefined)), 'Ada')
  assert.equal(nodeTitle(node('a', 'agent')), 'Agent')
  assert.equal(splitJoinedInputs('just one'), null)
  const joined = splitJoinedInputs('Input 1:\nfirst\n\nInput 2:\nsecond')
  assert.deepEqual(joined, [{ label: 'Input 1', body: 'first' }, { label: 'Input 2', body: 'second' }])
  assert.equal(nodeTitle(node('p', 'prompt', { text: '' })), 'Prompt')
  assert.equal(copyTitle('Weekly'), 'Copy of Weekly')
  const original = graph([makeNode('trigger', { x: 80, y: 80 })])
  const copy = cloneGraphNewIds(original, () => 'new')
  assert.notEqual(copy.nodes[0]?.id, original.nodes[0]?.id)
  assert.equal(copy.nodes[0]?.id, 'new')
})

test('a card that grows with its fields ignores a stored height', () => {
  const trigger = { ...node('t', 'trigger'), size: { width: 280, height: 158 } }
  assert.equal(fixedHeight(trigger), undefined)
  assert.deepEqual(nodeBox(trigger, 420), { x: 0, y: 0, width: 280, height: 420 })
  const note = { ...node('n', 'note'), size: { width: 220, height: 180 } }
  assert.equal(fixedHeight(note), 180)
  assert.equal(nodeBox(note, 400).height, 180)
  const shape = { ...node('s', 'shape'), size: { width: 120, height: 80 } }
  assert.equal(nodeBox(shape, 200).height, 80)
})
