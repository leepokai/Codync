import { edgeId, isNodeKind, isStep, type Branch, type Edge, type EditMode, type Graph, type Outline } from './workflow.ts'

/** Edges that return to a step still on the walk. Those don't wait on the upstream step. */
export function loopEdges(graph: Graph) {
  const outs = new Map<string, Edge[]>()
  const incoming = new Set<string>()
  for (const edge of graph.edges) {
    const list = outs.get(edge.from) ?? []
    list.push(edge)
    outs.set(edge.from, list)
    incoming.add(edge.to)
  }
  const back = new Set<string>()
  const state = new Map<string, number>()
  const walk = (id: string) => {
    state.set(id, 1)
    for (const edge of outs.get(id) ?? []) {
      const mark = state.get(edge.to) ?? 0
      if (mark === 1) back.add(edge.id)
      else if (mark === 0) walk(edge.to)
    }
    state.set(id, 2)
  }
  const starts = [...graph.nodes.map((node) => node.id).filter((id) => !incoming.has(id)), ...graph.nodes.map((node) => node.id)]
  for (const id of starts) if (!state.has(id)) walk(id)
  return back
}

/** Top to bottom: a layer's steps go left to right, then top to bottom. Back edges stay out of the order. */
export function readingOrder(graph: Graph) {
  const back = loopEdges(graph)
  const steps = graph.nodes.filter((node) => isStep(node.kind))
  const indegree = new Map(steps.map((node) => [node.id, 0]))
  const outs = new Map<string, string[]>()
  for (const edge of graph.edges) {
    if (back.has(edge.id)) continue
    if (!indegree.has(edge.to) || !indegree.has(edge.from)) continue
    indegree.set(edge.to, (indegree.get(edge.to) ?? 0) + 1)
    const list = outs.get(edge.from) ?? []
    list.push(edge.to)
    outs.set(edge.from, list)
  }
  const pos = (id: string) => {
    const node = steps.find((item) => item.id === id)
    return node ? [node.at.x, node.at.y, id] as const : [0, 0, id] as const
  }
  const byPos = (a: string, b: string) => {
    const [ax, ay, aid] = pos(a)
    const [bx, by, bid] = pos(b)
    return ax - bx || ay - by || (aid < bid ? -1 : aid > bid ? 1 : 0)
  }
  const ready = [...indegree.entries()].filter(([, count]) => count === 0).map(([id]) => id)
  const order: string[] = []
  while (ready.length) {
    ready.sort(byPos)
    const id = ready.shift()!
    order.push(id)
    for (const next of outs.get(id) ?? []) {
      const left = (indegree.get(next) ?? 0) - 1
      indegree.set(next, left)
      if (left === 0) ready.push(next)
    }
  }
  for (const node of steps) if (!order.includes(node.id)) order.push(node.id)
  return order
}

export function editMode(graph: Graph): EditMode {
  if (graph.nodes.some((node) => !isNodeKind(node.kind))) return 'readOnly'
  return outlineOf(graph) ? 'outline' : 'fields'
}

/** The phone's columns, or null when the lines have to stay as drawn. */
export function outlineOf(graph: Graph): Outline | null {
  if (graph.nodes.some((node) => !isNodeKind(node.kind))) return null
  const steps = graph.nodes.filter((node) => isStep(node.kind))
  const triggers = steps.filter((node) => node.kind === 'trigger')
  if (triggers.length !== 1 || steps.filter((node) => node.kind === 'condition').length > 1) return null
  const trigger = triggers[0]!.id
  const kinds = new Map(steps.map((node) => [node.id, node.kind]))
  const nodes = new Map(steps.map((node) => [node.id, node]))
  if (graph.edges.some((edge) => edge.type !== 'text' || !kinds.has(edge.from) || !kinds.has(edge.to))) return null
  if (loopEdges(graph).size) return null
  const reached = new Set<string>([trigger])
  const queue = [trigger]
  while (queue.length) {
    const id = queue.pop()!
    for (const edge of graph.edges) {
      if (edge.from === id && !reached.has(edge.to)) {
        reached.add(edge.to)
        queue.push(edge.to)
      }
    }
  }
  const prompts = new Map<string, string>()
  for (const node of steps) {
    if (node.kind !== 'prompt') continue
    const targets = graph.edges.filter((edge) => edge.from === node.id).map((edge) => edge.to)
    if (!targets.length || !targets.some((id) => reached.has(id))) continue
    if (targets.length !== 1 || !targets.every((id) => reached.has(id))) return null
    prompts.set(node.id, targets[0]!)
  }
  const loose = steps.map((node) => node.id).filter((id) => !reached.has(id) && !prompts.has(id))
  const looseSet = new Set(loose)
  if (graph.edges.some((edge) => looseSet.has(edge.from) && !looseSet.has(edge.to))) return null
  const core = new Set([...reached].filter((id) => kinds.get(id) !== 'output'))
  const chain = coreChain(trigger, core, kinds, graph.edges)
  if (!chain) return null
  const order = (left: string, right: string) => {
    const a = nodes.get(left)
    const b = nodes.get(right)
    const ax = a?.at.x ?? 0
    const ay = a?.at.y ?? 0
    const bx = b?.at.x ?? 0
    const by = b?.at.y ?? 0
    return ax - bx || ay - by
  }
  const placements = new Map<string, string[]>()
  const pushPlace = (key: string, id: string) => {
    const list = placements.get(key) ?? []
    list.push(id)
    placements.set(key, list)
  }
  for (const node of steps) {
    if (node.kind !== 'output' || !reached.has(node.id)) continue
    const feeds = graph.edges.filter((edge) => edge.to === node.id && core.has(edge.from))
    if (feeds.length === 1 && feeds[0]!.from === (chain.condition ?? '')) {
      const branch = feeds[0]!.branch
      if (!branch) return null
      pushPlace(`start:${branch}`, node.id)
    } else if (feeds.length === 1) {
      pushPlace(`after:${feeds[0]!.from}`, node.id)
    } else if (feeds.length === 2 && chain.condition) {
      const ends = [laneEnd(chain, 'yes'), laneEnd(chain, 'no')]
      const matches = (edge: Edge) => ends.some((end) => end.from === edge.from && end.branch === (edge.branch ?? null))
      if (!matches(feeds[0]!) || !matches(feeds[1]!) || (feeds[0]!.from === feeds[1]!.from && feeds[0]!.branch === feeds[1]!.branch)) return null
      pushPlace('start:after', node.id)
    } else {
      return null
    }
  }
  const withOutputs = (name: string, items: string[]) => {
    const result = [...(placements.get(`start:${name}`) ?? [])].sort(order)
    if (name === 'main') result.push(...[...(placements.get(`after:${trigger}`) ?? [])].sort(order))
    for (const id of items) {
      result.push(id)
      result.push(...[...(placements.get(`after:${id}`) ?? [])].sort(order))
    }
    return result
  }
  const outline: Outline = {
    trigger,
    main: withOutputs('main', chain.main),
    condition: chain.condition,
    yes: withOutputs('yes', chain.yes),
    no: withOutputs('no', chain.no),
    after: withOutputs('after', chain.after),
    loose,
  }
  const pairs = [...prompts.entries()].sort((a, b) => order(a[0], b[0]))
  for (const [prompt, target] of pairs) {
    if (target === chain.condition) {
      outline.main.push(prompt)
      continue
    }
    const list = [outline.main, outline.yes, outline.no, outline.after].find((column) => column.includes(target))
    if (!list) return null
    list.splice(list.indexOf(target), 0, prompt)
  }
  return outline
}

interface CoreChain {
  main: string[]
  condition: string | null
  yes: string[]
  no: string[]
  after: string[]
}

function laneEnd(chain: CoreChain, branch: Branch): { from: string; branch: Branch | null } {
  const list = branch === 'yes' ? chain.yes : chain.no
  const last = list[list.length - 1]
  return last ? { from: last, branch: null } : { from: chain.condition ?? '', branch }
}

function coreChain(trigger: string, core: Set<string>, kinds: Map<string, string>, edges: Edge[]): CoreChain | null {
  const out = new Map<string, Edge[]>()
  const incoming = new Map<string, number>()
  for (const edge of edges) {
    if (!core.has(edge.from) || !core.has(edge.to)) continue
    const list = out.get(edge.from) ?? []
    list.push(edge)
    out.set(edge.from, list)
    incoming.set(edge.to, (incoming.get(edge.to) ?? 0) + 1)
  }
  for (const id of core) {
    const ways = out.get(id) ?? []
    if (kinds.get(id) === 'condition') {
      if (ways.some((edge) => !edge.branch)) return null
      if (ways.filter((edge) => edge.branch === 'yes').length > 1) return null
      if (ways.filter((edge) => edge.branch === 'no').length > 1) return null
    } else if (ways.length > 1) {
      return null
    }
  }
  if (incoming.has(trigger)) return null
  const seen = new Set<string>([trigger])
  const next = (id: string) => out.get(id)?.[0]?.to
  const start = runColumn(next(trigger), true, seen, incoming, kinds, next)
  if (!start || start.rest) return null
  if (!start.condition) {
    return seen.size === core.size ? { main: start.list, condition: null, yes: [], no: [], after: [] } : null
  }
  const branch = (name: Branch) => out.get(start.condition!)?.find((edge) => edge.branch === name)?.to
  const yes = runColumn(branch('yes'), false, seen, incoming, kinds, next)
  const no = runColumn(branch('no'), false, seen, incoming, kinds, next)
  if (!yes || !no || yes.rest !== no.rest) return null
  const after: string[] = []
  if (yes.rest) {
    const join = yes.rest
    if (incoming.get(join) !== 2 || kinds.get(join) === 'condition' || seen.has(join)) return null
    seen.add(join)
    after.push(join)
    const rest = runColumn(next(join), false, seen, incoming, kinds, next)
    if (!rest || rest.rest) return null
    after.push(...rest.list)
  }
  return seen.size === core.size ? { main: start.list, condition: start.condition, yes: yes.list, no: no.list, after } : null
}

function runColumn(
  first: string | undefined,
  stopAtCondition: boolean,
  seen: Set<string>,
  incoming: Map<string, number>,
  kinds: Map<string, string>,
  next: (id: string) => string | undefined,
): { list: string[]; condition: string | null; rest: string | null } | null {
  const list: string[] = []
  let current = first
  while (current && (incoming.get(current) ?? 0) === 1) {
    if (seen.has(current)) return null
    seen.add(current)
    if (kinds.get(current) === 'condition') {
      if (!stopAtCondition) return null
      return { list, condition: current, rest: null }
    }
    list.push(current)
    current = next(current)
  }
  return { list, condition: null, rest: current ?? null }
}

/** Rewrites the lines of an outline and lays the steps out in columns. Notes stay where they are. */
export function graphFromOutline(graph: Graph, outline: Outline): Graph {
  const kinds = new Map(graph.nodes.map((node) => [node.id, node.kind]))
  const edges: Edge[] = []
  const link = (from: string, to: string, branch?: Branch | null) => {
    edges.push({ id: edgeId(from, to, branch), from, to, type: 'text', branch: branch ?? null })
  }
  interface Source { from: string; branch: Branch | null }
  const walk = (items: string[], start: Source[]) => {
    let sources = start
    const prompts: string[] = []
    for (const id of items) {
      if (kinds.get(id) === 'prompt') {
        prompts.push(id)
        continue
      }
      for (const source of sources) link(source.from, id, source.branch)
      for (const prompt of prompts) link(prompt, id, null)
      prompts.length = 0
      if (kinds.get(id) !== 'output') sources = [{ from: id, branch: null }]
    }
    return { sources, prompts }
  }
  const main = walk(outline.main, [{ from: outline.trigger, branch: null }])
  if (outline.condition) {
    for (const source of main.sources) link(source.from, outline.condition, source.branch)
    for (const prompt of main.prompts) link(prompt, outline.condition, null)
    const yes = walk(outline.yes, [{ from: outline.condition, branch: 'yes' }])
    const no = walk(outline.no, [{ from: outline.condition, branch: 'no' }])
    if (outline.after.length) walk(outline.after, [...yes.sources, ...no.sources])
  }
  const loose = new Set(outline.loose)
  edges.push(...graph.edges.filter((edge) => loose.has(edge.from) && loose.has(edge.to)))
  const next = structuredClone(graph)
  next.edges = edges
  placeOutline(next, outline)
  return next
}

function placeOutline(graph: Graph, outline: Outline) {
  const set = (id: string, column: number, y: number) => {
    const node = graph.nodes.find((item) => item.id === id)
    if (node) node.at = { x: 80 + column * 400, y }
  }
  const placeList = (items: string[], column: number, y: number) => {
    let next = column
    for (const id of items) {
      next += 1
      set(id, next, y)
    }
    return next
  }
  set(outline.trigger, 0, 80)
  let col = placeList(outline.main, 0, 80)
  if (!outline.condition) return
  col += 1
  set(outline.condition, col, 80)
  const yes = placeList(outline.yes, col, 80)
  const no = placeList(outline.no, col, 300)
  placeList(outline.after, Math.max(yes, no), 80)
}

