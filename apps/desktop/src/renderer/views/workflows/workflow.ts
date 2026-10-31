// The workflow graph the desktop canvas shares with the host. No React.
// Host validate (host/src/workflows/graph.rs) is authoritative on save.

export type NodeKind = 'trigger' | 'prompt' | 'agent' | 'condition' | 'approval' | 'output' | 'text' | 'note' | 'shape'
export type PortType = 'text'
export type Branch = 'yes' | 'no'
export type EditMode = 'outline' | 'fields' | 'script' | 'readOnly'
export type BotState = 'ok' | 'missing' | 'group'
export type RunStatus = 'running' | 'approval' | 'stopping' | 'done' | 'failed' | 'stopped'
export type StepStatus = 'running' | 'approval' | 'done' | 'failed' | 'stopped' | 'skipped'

export interface Point { x: number; y: number }
export interface Size { width: number; height: number }

export interface WorkflowSchedule {
  type: 'cron'
  expression: string
  timeZone: string
}

export type FieldValue = string | number | boolean | null
export type Fields = Record<string, FieldValue>

export interface Node {
  id: string
  kind: string
  at: Point
  size?: Size | null
  locked: boolean
  fields: Fields
  schedule?: WorkflowSchedule | null
}

export interface Edge {
  id: string
  from: string
  to: string
  /** Wire key is `type`. */
  type: PortType
  branch?: Branch | null
}

export interface Stroke {
  id: string
  brush: 'pen' | 'marker' | string
  color: string
  width: number
  points: Point[]
}

export interface Graph {
  nodes: Node[]
  edges: Edge[]
  strokes: Stroke[]
}

export interface WorkflowSummary {
  id: string
  title: string
  revision: number
  enabled: boolean
  updatedAt: number
  running?: RunStatus | null
  lastRun?: { number: number; status: string; endedAt?: number | null } | null
}

export interface WorkflowWebhook {
  nodeId: string
  url: string
  secret?: string
}

export interface WorkflowDetail extends WorkflowSummary {
  createdAt: number
  graph: Graph
  source?: 'graph' | 'script'
  script?: string
  phases?: string[]
  editMode?: EditMode
  webhooks?: WorkflowWebhook[]
}

export interface WorkflowVersion {
  id: string
  revision: number
  kind: 'auto' | 'restore' | 'run'
  createdAt: number
}

export interface RunSummary {
  id: string
  number: number
  source: string
  status: string
  startedAt: number
  endedAt?: number | null
  error?: string | null
}

export interface WorkflowStep {
  id: string
  nodeId: string
  attempt: number
  requestId: string
  kind: string
  botId?: string | null
  status: string
  input: string
  output?: string | null
  branch?: Branch | null
  passes: number
  truncated: boolean
  error?: string | null
  startedAt?: number | null
  endedAt?: number | null
  title: string
}

export interface RunDetail extends RunSummary {
  graph: Graph
  steps: WorkflowStep[]
}

export interface Outline {
  trigger: string
  main: string[]
  condition?: string | null
  yes: string[]
  no: string[]
  after: string[]
  loose: string[]
}

export interface GraphProblem {
  kind: string
  message: string
}

export const NODE_KINDS: NodeKind[] = ['trigger', 'prompt', 'agent', 'condition', 'approval', 'output', 'text', 'note', 'shape']
export const NOTE_TONES = ['yellow', 'green', 'blue', 'violet', 'orange', 'gray'] as const
export const SHAPES = ['rectangle', 'ellipse', 'diamond', 'triangle'] as const
export const ALIGNS = ['leading', 'center', 'trailing'] as const
export const MAX_NODES = 200
export const MAX_EDGES = 400
export const MAX_STROKES = 500
export const MAX_STROKE_POINTS = 2000
export const MAX_FIELD_KEYS = 32
export const MAX_KEY_LEN = 64
export const MAX_FIELD_CHARS = 20_000
export const MAX_TITLE = 160
export const MAX_GRAPH_BYTES = 1024 * 1024
export const MAX_COORD = 1_000_000
export const MAX_SIZE = 100_000

export const isNodeKind = (kind: string): kind is NodeKind => (NODE_KINDS as string[]).includes(kind)
export const isAnnotation = (kind: string) => kind === 'text' || kind === 'note' || kind === 'shape'
export const isStep = (kind: string) => isNodeKind(kind) && !isAnnotation(kind)

/** What a node accepts and what it emits. Notes have neither. */
export function ports(kind: string): [PortType[], PortType[]] {
  switch (kind) {
    case 'trigger':
    case 'prompt':
      return [[], ['text']]
    case 'agent':
    case 'condition':
    case 'approval':
      return [['text'], ['text']]
    case 'output':
      return [['text'], []]
    default:
      return [[], []]
  }
}

export const newId = () => crypto.randomUUID()

export function fieldString(fields: Fields, key: string) {
  const value = fields[key]
  return typeof value === 'string' ? value : ''
}

function firstLine(text: string) {
  const line = text.trim().split('\n')[0]?.trim() ?? ''
  return line
}

/** The name a card and a step list show. An assistant or a chat output uses the bot's name. */
export function nodeTitle(node: Node, botName?: (id: string) => string | undefined) {
  switch (node.kind) {
    case 'trigger':
      return node.fields.trigger === 'schedule' ? 'Schedule' : node.fields.trigger === 'webhook' ? 'Webhook' : 'Manual'
    case 'prompt':
      return firstLine(fieldString(node.fields, 'text')) || 'Prompt'
    case 'agent': {
      const id = fieldString(node.fields, 'botId').trim()
      return (id && botName?.(id)) || 'Assistant'
    }
    case 'condition':
      return 'Condition'
    case 'approval':
      return firstLine(fieldString(node.fields, 'question')) || 'Approval'
    case 'output': {
      if (node.fields.target === 'chat') {
        const id = fieldString(node.fields, 'botId').trim()
        return (id && botName?.(id)) || 'Notification'
      }
      return 'Notification'
    }
    default:
      return 'Note'
  }
}

/** Running, Needs you, Done, Failed, Stopped — or nothing when this workflow has not run. */
export function runStatusLabel(status: string | null | undefined) {
  switch (status) {
    case 'running':
    case 'stopping':
      return 'Running'
    case 'approval':
      return 'Needs you'
    case 'done':
      return 'Done'
    case 'failed':
      return 'Failed'
    case 'stopped':
      return 'Stopped'
    default:
      return ''
  }
}

export function summaryLine(summary: WorkflowSummary | undefined) {
  if (!summary) return ''
  return runStatusLabel(summary.running) || runStatusLabel(summary.lastRun?.status)
}

const DEFAULT_WIDTH: Record<string, number> = { prompt: 296, agent: 350, condition: 320, note: 220 }
const DEFAULT_HEIGHT: Record<string, number> = {
  trigger: 168, prompt: 180, agent: 220, condition: 176, approval: 150, output: 150, text: 96, note: 160, shape: 140,
}

export function nodeBox(node: Node, measuredHeight?: number) {
  const width = node.size?.width ?? DEFAULT_WIDTH[node.kind] ?? 280
  const height = node.size?.height ?? measuredHeight ?? DEFAULT_HEIGHT[node.kind] ?? 150
  return { x: node.at.x, y: node.at.y, width, height }
}

export interface ResizeLimit { minWidth: number; maxWidth: number; minHeight?: number; maxHeight?: number }

export function resizeLimit(kind: string): ResizeLimit {
  if (kind === 'trigger') return { minWidth: 240, maxWidth: 480 }
  if (kind === 'condition') return { minWidth: 260, maxWidth: 560 }
  if (kind === 'note') return { minWidth: 160, maxWidth: 480, minHeight: 120, maxHeight: 480 }
  return { minWidth: 200, maxWidth: 640 }
}

export function edgeId(from: string, to: string, branch?: Branch | null) {
  return branch ? `${from}.${branch}>${to}:text` : `${from}>${to}:text`
}

export function makeNode(kind: NodeKind, at: Point, id = newId()): Node {
  const fields: Fields = {}
  if (kind === 'trigger') fields.trigger = 'manual'
  if (kind === 'prompt' || kind === 'text' || kind === 'note' || kind === 'shape') fields.text = ''
  if (kind === 'agent') { fields.botId = ''; fields.task = '' }
  if (kind === 'condition') { fields.op = 'contains'; fields.value = '' }
  if (kind === 'approval') fields.question = ''
  if (kind === 'output') fields.target = 'notify'
  if (kind === 'text') { fields.font = 16; fields.align = 'leading'; fields.bold = false }
  if (kind === 'note') fields.tone = 'yellow'
  if (kind === 'shape') { fields.shape = 'rectangle'; fields.fill = 'gray' }
  return { id, kind, at, locked: false, fields, schedule: null }
}

export function emptyGraph(): Graph {
  return { nodes: [], edges: [], strokes: [] }
}

/** `Copy of {title}`, cut to the title limit. */
export function copyTitle(title: string) {
  const next = `Copy of ${title}`.trim()
  return (next.length > MAX_TITLE ? next.slice(0, MAX_TITLE).trim() : next) || 'Workflow'
}

/** A copy whose node, edge and stroke ids are all new, so pins and drafts stay on the original. */
export function cloneGraphNewIds(graph: Graph, titleIds: () => string = newId): Graph {
  const map = new Map(graph.nodes.map((node) => [node.id, titleIds()]))
  return {
    nodes: graph.nodes.map((node) => ({ ...structuredClone(node), id: map.get(node.id)! })),
    edges: graph.edges.map((edge) => {
      const from = map.get(edge.from) ?? edge.from
      const to = map.get(edge.to) ?? edge.to
      return { ...edge, id: edgeId(from, to, edge.branch), from, to }
    }),
    strokes: graph.strokes.map((stroke) => ({ ...structuredClone(stroke), id: titleIds() })),
  }
}

export function searchText(node: Node, botName?: (id: string) => string | undefined) {
  return [nodeTitle(node, botName), fieldString(node.fields, 'task'), fieldString(node.fields, 'text'), fieldString(node.fields, 'question')]
    .join('\n')
    .toLowerCase()
}

/** Why a drag between two ports cannot land, or null when the line is allowed (a loop included). */
export function connectionProblem(graph: Graph, fromId: string, toId: string, branch?: Branch | null): string | null {
  if (fromId === toId) return "A step can't connect to itself"
  const from = graph.nodes.find((node) => node.id === fromId)
  const to = graph.nodes.find((node) => node.id === toId)
  if (!from || !to) return "A line points at a step that isn't in this workflow"
  if (isAnnotation(from.kind) || isAnnotation(to.kind)) return "A line can't connect a note"
  const [, outs] = ports(from.kind)
  const [ins] = ports(to.kind)
  if (!outs.length || !ins.length) return "That line doesn't fit this step"
  if (from.kind === 'condition' && !branch) return 'A condition line needs a yes or no'
  if (from.kind !== 'condition' && branch) return 'Only a condition line has a yes or no'
  if (from.kind === 'condition' && graph.edges.some((edge) => edge.from === fromId && edge.branch === branch)) {
    return `This condition already has a ${branch} line`
  }
  if (graph.edges.some((edge) => edge.from === fromId && edge.to === toId && (edge.branch ?? null) === (branch ?? null))) {
    return 'Those steps are already connected'
  }
  return null
}


export { validate } from './validate.ts'
export { editMode, graphFromOutline, loopEdges, outlineOf, readingOrder } from './outline.ts'

/** Steps that count toward a run's progress. */
export function runnableNodes(graph: Graph) {
  return graph.nodes.filter((node) => isStep(node.kind))
}

export function latestStepByNode(steps: WorkflowStep[]) {
  const byNode = new Map<string, WorkflowStep>()
  for (const step of steps) {
    const held = byNode.get(step.nodeId)
    if (!held || step.attempt >= held.attempt) byNode.set(step.nodeId, step)
  }
  return byNode
}

export function progressOf(graph: Graph, steps: Iterable<WorkflowStep>) {
  const total = runnableNodes(graph).length
  const settled = new Set(['done', 'failed', 'skipped', 'stopped'])
  let done = 0
  for (const step of latestStepByNode([...steps]).values()) if (settled.has(step.status)) done += 1
  return { done, total }
}

export function runClock(startedAt: number, endedAt: number | null | undefined, now = Date.now()) {
  const end = endedAt ?? now
  const seconds = Math.max(0, Math.round((end - startedAt) / 1000))
  if (seconds < 60) return `${seconds}s`
  const minutes = Math.floor(seconds / 60)
  const rest = seconds % 60
  if (minutes < 60) return rest ? `${minutes}m ${rest}s` : `${minutes}m`
  const hours = Math.floor(minutes / 60)
  return `${hours}h ${minutes % 60}m`
}

/** A time today, otherwise a short date. */
export function runWhen(startedAt: number, now = Date.now()) {
  const date = new Date(startedAt)
  const today = new Date(now)
  if (date.toDateString() === today.toDateString()) return date.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })
  return date.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' })
}
