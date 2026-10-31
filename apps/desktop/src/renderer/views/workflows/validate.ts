import {
  ALIGNS, MAX_COORD, MAX_EDGES, MAX_FIELD_CHARS, MAX_FIELD_KEYS, MAX_GRAPH_BYTES, MAX_KEY_LEN, MAX_NODES, MAX_SIZE,
  MAX_STROKE_POINTS, MAX_STROKES, NOTE_TONES, SHAPES,
  fieldString, isAnnotation, isNodeKind, ports,
  type BotState, type Graph, type GraphProblem, type Node, type NodeKind,
} from './workflow.ts'

const KIND_FIELDS: Record<NodeKind, string[]> = {
  trigger: ['trigger', 'timezone'],
  prompt: ['text'],
  agent: ['botId', 'task'],
  condition: ['op', 'value'],
  approval: ['question'],
  output: ['target', 'botId'],
  text: ['text', 'font', 'align', 'bold'],
  note: ['text', 'tone'],
  shape: ['shape', 'text', 'fill'],
}

const problem = (kind: string, message: string): GraphProblem => ({ kind, message })

/** The first reason a graph cannot be saved, or null when the host would accept the shape. */
export function validate(graph: Graph, bots: (id: string) => BotState = () => 'ok'): GraphProblem | null {
  let bytes = 0
  try {
    bytes = new TextEncoder().encode(JSON.stringify(graph)).length
  } catch {
    return problem('limit', 'This workflow is too large to save')
  }
  if (bytes > MAX_GRAPH_BYTES) return problem('limit', 'This workflow is too large to save')
  if (graph.nodes.length > MAX_NODES) return problem('limit', 'This workflow has too many steps')
  if (graph.edges.length > MAX_EDGES) return problem('limit', 'This workflow has too many lines')
  if (graph.strokes.length > MAX_STROKES) return problem('limit', 'This workflow has too many drawings')
  const ids = new Set<string>()
  for (const node of graph.nodes) {
    if (ids.has(node.id)) return problem('duplicate', 'Two steps share an id')
    ids.add(node.id)
    const point = checkPoint(node.at.x, node.at.y)
    if (point) return point
    if (node.size) {
      const width = checkSize(node.size.width)
      if (width) return width
      const height = checkSize(node.size.height)
      if (height) return height
    }
    const fields = checkFields(node, bots)
    if (fields) return fields
  }
  const byId = new Map(graph.nodes.map((node) => [node.id, node]))
  const seen = new Set<string>()
  const branches = new Map<string, number>()
  for (const edge of graph.edges) {
    if (seen.has(edge.id)) return problem('duplicate', 'Two lines share an id')
    seen.add(edge.id)
    if (edge.from === edge.to) return problem('port', "A step can't connect to itself")
    const from = byId.get(edge.from)
    const to = byId.get(edge.to)
    if (!from || !to) return problem('port', "A line points at a step that isn't in this workflow")
    if (isAnnotation(from.kind) || isAnnotation(to.kind)) return problem('annotation', "A line can't connect a note")
    if (edge.type !== 'text') return problem('port', "That line doesn't fit this step")
    const [, outs] = ports(from.kind)
    const [ins] = ports(to.kind)
    if (!outs.length || !ins.length) return problem('port', "That line doesn't fit this step")
    if (from.kind === 'condition') {
      if (!edge.branch) return problem('branch', 'A condition line needs a yes or no')
      const key = `${from.id}:${edge.branch}`
      const count = (branches.get(key) ?? 0) + 1
      branches.set(key, count)
      if (count > 1) return problem('condition-branch', `This condition already has a ${edge.branch} line`)
    } else if (edge.branch) {
      return problem('branch', 'Only a condition line has a yes or no')
    }
    const copies = graph.edges.filter((other) => other.from === edge.from && other.to === edge.to && (other.branch ?? null) === (edge.branch ?? null))
    if (copies.length !== 1) return problem('duplicate', 'Those steps are already connected')
  }
  for (const stroke of graph.strokes) {
    if (!stroke.id || stroke.points.length > MAX_STROKE_POINTS) return problem('limit', 'A drawing has too many points')
    if (stroke.brush !== 'pen' && stroke.brush !== 'marker') return problem('field', "A drawing's brush is pen or marker")
    const width = checkSize(stroke.width)
    if (width) return width
    for (const point of stroke.points) {
      const bad = checkPoint(point.x, point.y)
      if (bad) return bad
    }
  }
  return null
}

function checkPoint(x: number, y: number): GraphProblem | null {
  if (!Number.isFinite(x) || !Number.isFinite(y) || Math.abs(x) > MAX_COORD || Math.abs(y) > MAX_COORD) {
    return problem('limit', 'A step is too far from the canvas')
  }
  return null
}

function checkSize(value: number): GraphProblem | null {
  if (!Number.isFinite(value) || value < 0 || value > MAX_SIZE) return problem('limit', 'A step is too large')
  return null
}

function checkFields(node: Node, bots: (id: string) => BotState): GraphProblem | null {
  const keys = Object.keys(node.fields)
  if (keys.length > MAX_FIELD_KEYS) return problem('limit', 'A step has too many settings')
  for (const key of keys) {
    if (key.length > MAX_KEY_LEN) return problem('field', 'unknown field')
    const value = node.fields[key]
    if (typeof value === 'string' && [...value].length > MAX_FIELD_CHARS) return problem('limit', "A step's text is too long")
  }
  if (!isNodeKind(node.kind)) return problem('field', 'unknown field')
  const allowed = KIND_FIELDS[node.kind]
  for (const key of keys) if (!allowed.includes(key)) return problem('field', 'unknown field')
  switch (node.kind) {
    case 'trigger':
      return checkTrigger(node)
    case 'agent':
      return checkBot(fieldString(node.fields, 'botId'), bots, false)
    case 'condition':
      return checkCondition(node)
    case 'output':
      return checkOutput(node, bots)
    case 'text':
      return checkText(node)
    case 'note':
      return checkTone(fieldString(node.fields, 'tone'))
    case 'shape':
      if (!(SHAPES as readonly string[]).includes(fieldString(node.fields, 'shape'))) return problem('field', 'Pick a shape')
      return checkTone(fieldString(node.fields, 'fill'))
    default:
      return null
  }
}

function checkTrigger(node: Node): GraphProblem | null {
  const kind = fieldString(node.fields, 'trigger')
  if (kind !== 'manual' && kind !== 'schedule' && kind !== 'webhook') return problem('field', 'Pick how this workflow starts')
  if (kind === 'manual' || kind === 'webhook') {
    if (node.schedule) return problem('schedule', 'Only a schedule trigger has a time')
    return null
  }
  const schedule = node.schedule
  if (!schedule || schedule.type !== 'cron' || !schedule.expression.trim()) return problem('schedule', 'Pick when this workflow runs')
  if (fieldString(node.fields, 'timezone') !== schedule.timeZone) return problem('schedule', "The time zone doesn't match the schedule")
  if (schedule.expression.trim().split(/\s+/).length < 5) return problem('schedule', "That schedule isn't valid")
  return null
}

function checkBot(id: string, bots: (id: string) => BotState, required: boolean): GraphProblem | null {
  const trimmed = id.trim()
  if (!trimmed) return required ? problem('field', 'Choose a bot for this step') : null
  const state = bots(trimmed)
  if (state === 'group') return problem('field', "A group can't run this step")
  if (state === 'missing') return problem('field', 'Choose a bot for this step')
  return null
}

function checkCondition(node: Node): GraphProblem | null {
  const op = fieldString(node.fields, 'op')
  if (op === 'contains' || op === 'equals' || op === 'empty') return null
  if (op === 'regex') {
    try {
      new RegExp(fieldString(node.fields, 'value'))
      return null
    } catch {
      return problem('field', "That pattern isn't valid")
    }
  }
  return problem('field', 'Pick how this condition matches')
}

function checkOutput(node: Node, bots: (id: string) => BotState): GraphProblem | null {
  const target = fieldString(node.fields, 'target')
  if (target === 'notify') return null
  if (target === 'chat') return checkBot(fieldString(node.fields, 'botId'), bots, true)
  return problem('field', 'Pick where this step sends its result')
}

function checkText(node: Node): GraphProblem | null {
  const font = node.fields.font
  const size = typeof font === 'number' ? font : 16
  if (size < 8 || size > 96) return problem('field', 'Text size is between 8 and 96')
  const align = fieldString(node.fields, 'align') || 'leading'
  if (!(ALIGNS as readonly string[]).includes(align)) return problem('field', 'Pick how the text lines up')
  if ('bold' in node.fields && typeof node.fields.bold !== 'boolean') return problem('field', 'Bold is on or off')
  return null
}

function checkTone(tone: string): GraphProblem | null {
  if (tone && !(NOTE_TONES as readonly string[]).includes(tone)) return problem('field', 'Pick a color')
  return null
}

