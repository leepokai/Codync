import { useEffect, useRef, useState, type CSSProperties, type MouseEvent as ReactMouseEvent, type PointerEvent as ReactPointerEvent } from 'react'
import { Button, SearchField } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { AnchoredMenu, type MenuItem } from '../../components/Overlay'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { reduceMotion } from '../../lib/theme'
import { useStore } from '../../store/context'
import { WorkflowAddMenu } from './WorkflowAddMenu'
import { edgeDraws, WorkflowEdges } from './WorkflowEdges'
import { WorkflowMinimap } from './WorkflowMinimap'
import { WorkflowNodes } from './WorkflowNodes'
import { WorkflowTools } from './WorkflowTools'
import { WorkflowZoom } from './WorkflowZoom'
import { fitCamera, fitDuration, lerpCamera, SNAP, snap, viewBounds, wheelCamera, worldPoint, zoomAt, zoomStep, type Camera } from './camera'
import { pasteClip, rememberClip } from './clipboard'
import { edgePath } from './edges'
import { addConnection, addNode, addStroke, deleteSelection, duplicateNodes, eraseStrokes, moveNodes, nudgeNodes, replaceNode, spreadOverlaps } from './edit'
import { keyAction, type Tool } from './keys'
import { chord } from './WorkflowTip'
import { edgeId, newId, nodeBox, ports, resizeLimit, searchText, type Branch, type Graph, type Node, type Point, type WorkflowStep } from './workflow'
import './workflows.css'

type Phase = 'begin' | 'change' | 'end' | 'text'
type Size = { width: number; height: number }

interface MenuAt {
  x: number
  y: number
  at: Point
  link?: { from: string; branch?: Branch | null }
}

export function WorkflowCanvas({ workflowId, graph, readOnly, scripted, ready, steps, now, onEdit, onUndo, onRedo, onRun, onRunFrom, onRunStep, onRetry, onDecide, onStopStep, onChrome }: {
  workflowId: string
  graph: Graph
  readOnly: boolean
  scripted?: boolean
  ready: boolean
  steps: Map<string, WorkflowStep>
  now: number
  onEdit: (graph: Graph, phase: Phase) => void
  onUndo: () => void
  onRedo: () => void
  onRun: () => void
  onRunFrom: (nodeId: string) => void
  onRunStep: (nodeId: string) => void
  onRetry: (stepId: string) => void
  onDecide: (stepId: string, approved: boolean) => void
  onStopStep: (stepId: string) => void
  onChrome?: (shown: boolean) => void
}) {
  const store = useStore()
  const tr = useTr()
  const viewport = useRef<HTMLDivElement>(null)
  const graphRef = useRef(graph)
  const cameraRef = useRef<Camera>({ x: 0, y: 0, scale: 1 })
  const moved = useRef(false)
  const fitted = useRef<string | null>(null)
  const space = useRef(false)
  const heights = useRef(new Map<string, number>())
  const gesture = useRef<Gesture | null>(null)
  const [camera, setCamera] = useState<Camera>({ x: 40, y: 40, scale: 1 })
  const [size, setSize] = useState<Size>({ width: 0, height: 0 })
  const [tool, setTool] = useState<Tool>('select')
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [selectedEdge, setSelectedEdge] = useState<string | null>(null)
  const [query, setQuery] = useState<string | null>(null)
  const [menu, setMenu] = useState<MenuAt | null>(null)
  const [canvasMenu, setCanvasMenu] = useState<MenuAt | null>(null)
  const [chrome, setChrome] = useState(true)
  const [draft, setDraft] = useState<string | null>(null)
  const [marquee, setMarquee] = useState<{ x: number; y: number; width: number; height: number } | null>(null)
  const [minimap, setMinimap] = useState(false)
  const [hint, setHint] = useState<string | null>(null)
  const [, setMeasured] = useState(0)
  graphRef.current = graph
  cameraRef.current = camera

  useEffect(() => {
    const node = viewport.current
    if (!node) return
    const observer = new ResizeObserver(() => setSize({ width: node.clientWidth, height: node.clientHeight }))
    observer.observe(node)
    setSize({ width: node.clientWidth, height: node.clientHeight })
    const onWheel = (event: WheelEvent) => {
      event.preventDefault()
      moved.current = true
      const rect = node.getBoundingClientRect()
      setCamera(wheelCamera(cameraRef.current, event, { x: event.clientX - rect.left, y: event.clientY - rect.top }))
    }
    node.addEventListener('wheel', onWheel, { passive: false })
    return () => {
      observer.disconnect()
      node.removeEventListener('wheel', onWheel)
    }
  }, [])

  useEffect(() => {
    if (fitted.current && fitted.current !== workflowId) moved.current = false
    if (!ready || size.width < 2 || moved.current || fitted.current === workflowId) return
    fitted.current = workflowId
    glide(fitCamera(graph.nodes.map((node) => nodeBox(node, heights.current.get(node.id))), size), setCamera, cameraRef)
  }, [ready, workflowId, size, graph.nodes.length])

  const boxes = new Map(graph.nodes.map((node) => [node.id, nodeBox(node, heights.current.get(node.id))]))
  const anchors = anchorMap(graph, boxes)
  const draws = edgeDraws(graph, anchors, steps)
  const grid = {
    '--workflow-x': `${camera.x}px`,
    '--workflow-y': `${camera.y}px`,
    '--workflow-grid': `${20 * camera.scale}px`,
  } as CSSProperties

  const publish = (next: Graph, phase: Phase) => {
    graphRef.current = next
    onEdit(next, phase)
  }
  const commit = (next: Graph) => {
    publish(next, 'begin')
    publish(next, 'end')
  }
  const world = (event: { clientX: number; clientY: number }): Point => {
    const rect = viewport.current?.getBoundingClientRect()
    return worldPoint(cameraRef.current, { x: event.clientX, y: event.clientY }, { x: rect?.left ?? 0, y: rect?.top ?? 0 })
  }
  const center = (): Point => {
    const view = viewBounds(cameraRef.current, size)
    return { x: view.x + view.width / 2, y: view.y + view.height / 2 }
  }

  const onKey = (event: React.KeyboardEvent) => {
    const field = event.target instanceof HTMLElement && event.target.closest('input, textarea')
    if (event.key === 'Escape') {
      setQuery(null)
      setMenu(null)
      return
    }
    if (event.key === ' ' && !field) {
      space.current = event.type === 'keydown'
      if (event.type === 'keydown') event.preventDefault()
      return
    }
    if (event.type !== 'keydown' || field) return
    const action = keyAction(event)
    if (!action) return
    event.preventDefault()
    if (readOnly && action.type !== 'zoom' && action.type !== 'fit' && action.type !== 'search' && action.type !== 'tool' && action.type !== 'chrome') return
    if (action.type === 'tool') setTool(action.tool)
    if (action.type === 'search') { reveal(); setQuery('') }
    if (action.type === 'chrome') toggleChrome()
    if (action.type === 'zoom') zoom(action.direction)
    if (action.type === 'fit') fit()
    if (action.type === 'run') onRun()
    if (action.type === 'undo') onUndo()
    if (action.type === 'redo') onRedo()
    if (action.type === 'select-all') setSelected(new Set(graph.nodes.filter((node) => !node.locked).map((node) => node.id)))
    if (action.type === 'delete') commit(deleteSelection(graph, selected, new Set(selectedEdge ? [selectedEdge] : [])))
    if (action.type === 'nudge') commit(nudgeNodes(graph, selected, action.dx, action.dy))
    if (action.type === 'copy' || action.type === 'cut') copySelection(action.type === 'cut')
    if (action.type === 'paste') void paste()
    if (action.type === 'duplicate') duplicate()
  }

  function reveal() {
    if (chrome) return
    setChrome(true)
    onChrome?.(true)
  }
  function toggleChrome() {
    const next = !chrome
    setChrome(next)
    onChrome?.(next)
    if (!next) setQuery(null)
  }
  const onContextMenu = (event: ReactMouseEvent) => {
    event.preventDefault()
    const target = event.target instanceof Element ? event.target : null
    if (target?.closest('[data-node], [data-edge], button, input, textarea, .workflow-tools, .workflow-zoom, .workflow-minimap, .workflow-search, .workflow-empty, .workflow-chrome-back')) return
    setCanvasMenu({ x: event.clientX, y: event.clientY, at: world(event) })
  }
  const canvasItems = (): MenuItem[] => [
    ...(!readOnly && !scripted ? [{
      title: 'Add node',
      icon: 'plus',
      action: () => { if (canvasMenu) setMenu({ x: canvasMenu.x, y: canvasMenu.y, at: canvasMenu.at }) },
    }] : []),
    { title: 'Select all', icon: 'cursorarrow', detail: chord('mod', 'A'), detailBeside: true, action: () => setSelected(new Set(graph.nodes.filter((node) => !node.locked).map((node) => node.id))) },
    { title: 'Search', icon: 'magnifyingglass', detail: chord('mod', 'F'), detailBeside: true, action: () => { reveal(); setQuery('') } },
    { title: 'Zoom to fit', icon: 'viewfinder', detail: chord('⇧', '1'), detailBeside: true, action: () => fit() },
    { title: chrome ? 'Hide interface' : 'Show interface', icon: chrome ? 'eye.slash' : 'eye', detail: chord('mod', '/'), detailBeside: true, action: toggleChrome },
  ]

  function zoom(direction: 1 | -1 | 0) {
    moved.current = true
    const anchor = { x: size.width / 2, y: size.height / 2 }
    const current = cameraRef.current
    setCamera(direction === 0 ? zoomAt(current, anchor, 1 / current.scale) : zoomStep(current, anchor, direction))
  }
  function fit() {
    moved.current = true
    glide(fitCamera([...boxes.values()], size), setCamera, cameraRef)
  }
  function copySelection(cut: boolean) {
    const nodes = graph.nodes.filter((node) => selected.has(node.id))
    const edges = graph.edges.filter((edge) => selected.has(edge.from) && selected.has(edge.to))
    if (nodes.length) rememberClip(nodes, edges)
    if (cut) commit(deleteSelection(graph, selected, new Set()))
  }
  function duplicate() {
    const next = duplicateNodes(graph, selected)
    commit(next.graph)
    setSelected(new Set(next.ids))
  }
  async function paste() {
    const clip = await pasteClip()
    if (!clip || readOnly) return
    const map = new Map(clip.nodes.map((node) => [node.id, newId()]))
    const nodes = clip.nodes.map((node) => ({ ...structuredClone(node), id: map.get(node.id)!, locked: false, at: { x: snap(node.at.x + SNAP), y: snap(node.at.y + SNAP) } }))
    const edges = clip.edges.flatMap((edge) => {
      const from = map.get(edge.from)
      const to = map.get(edge.to)
      if (!from || !to) return []
      return [{ ...edge, id: edgeId(from, to, edge.branch), from, to }]
    })
    commit({ ...graphRef.current, nodes: [...graphRef.current.nodes, ...nodes], edges: [...graphRef.current.edges, ...edges] })
    setSelected(new Set(nodes.map((node) => node.id)))
  }

  const onPointerDown = (event: ReactPointerEvent) => {
    if (event.button !== 0 && event.button !== 1) return
    const target = event.target as HTMLElement
    if (target.closest('.workflow-tools, .workflow-zoom, .workflow-minimap, .workflow-search, .workflow-empty')) return
    viewport.current?.focus()
    const point = world(event)
    if (target.closest('[data-port="out"]')) {
      const port = target.closest('[data-port]') as HTMLElement
      const from = port.dataset.node ?? ''
      const branch = (port.dataset.branch || null) as Branch | null
      gesture.current = { kind: 'connect', from, branch, at: point }
      return
    }
    if (target.closest('button, input, textarea, [role="switch"], [role="radio"]')) {
      const node = target.closest('[data-node]') as HTMLElement | null
      if (node?.dataset.node) choose(node.dataset.node, event.shiftKey)
      return
    }
    const resize = target.closest('[data-resize]') as HTMLElement | null
    if (resize?.dataset.resize && !readOnly) {
      const id = resize.dataset.resize
      const node = graph.nodes.find((item) => item.id === id)
      if (!node || node.locked) return
      const box = boxes.get(id)
      gesture.current = { kind: 'resize', id, x: event.clientX, y: event.clientY, width: box?.width ?? 280, height: box?.height ?? 150, moved: false }
      return
    }
    const node = target.closest('[data-node]') as HTMLElement | null
    const edge = target.closest('[data-edge]') as SVGPathElement | null
    if (space.current || tool === 'pan' || event.button === 1) {
      gesture.current = { kind: 'pan', x: event.clientX, y: event.clientY }
      return
    }
    if (!readOnly && (tool === 'pen' || tool === 'marker')) {
      const width = (tool === 'pen' ? 2.5 : 14) / camera.scale
      gesture.current = { kind: 'draw', id: newId(), brush: tool, width, points: [point], moved: false }
      return
    }
    if (!readOnly && tool === 'eraser') {
      gesture.current = { kind: 'erase', moved: false }
      return
    }
    if (!readOnly && (tool === 'note' || tool === 'shape' || tool === 'text')) {
      commit(addNode(graph, tool, point))
      setTool('select')
      return
    }
    if (edge?.dataset.edge) {
      setSelectedEdge(edge.dataset.edge)
      setSelected(new Set())
      return
    }
    if (node?.dataset.node) {
      const id = node.dataset.node
      const ids = choose(id, event.shiftKey)
      const origins = new Map<string, Point>()
      for (const item of graph.nodes) if (ids.has(item.id)) origins.set(item.id, { ...item.at })
      gesture.current = { kind: 'move', ids, origins, x: event.clientX, y: event.clientY, moved: false }
      return
    }
    gesture.current = { kind: 'marquee', x: event.clientX, y: event.clientY, add: event.shiftKey, base: new Set(event.shiftKey ? selected : []) }
    if (!event.shiftKey) setSelected(new Set())
    setSelectedEdge(null)
  }

  const onPointerMove = (event: ReactPointerEvent) => {
    const current = gesture.current
    if (!current) return
    const point = world(event)
    if (current.kind === 'pan') {
      moved.current = true
      const dx = event.clientX - current.x
      const dy = event.clientY - current.y
      current.x = event.clientX
      current.y = event.clientY
      setCamera({ ...cameraRef.current, x: cameraRef.current.x + dx, y: cameraRef.current.y + dy })
    } else if (current.kind === 'move' && !readOnly) {
      const dx = (event.clientX - current.x) / camera.scale
      const dy = (event.clientY - current.y) / camera.scale
      if (!current.moved && Math.hypot(dx, dy) < 2) return
      publish(moveNodes(graphRef.current, current.origins, dx, dy), current.moved ? 'change' : 'begin')
      current.moved = true
    } else if (current.kind === 'connect') {
      const from = anchors.get(current.from)?.outs.get(current.branch ?? '')
      if (from) setDraft(edgePath(from, point))
    } else if (current.kind === 'draw') {
      const last = current.points[current.points.length - 1]
      if (!last || Math.hypot(point.x - last.x, point.y - last.y) >= SAMPLE / camera.scale) current.points.push(point)
      current.moved = true
    } else if (current.kind === 'erase') {
      publish(eraseStrokes(graphRef.current, point, ERASER / camera.scale), current.moved ? 'change' : 'begin')
      current.moved = true
    } else if (current.kind === 'resize') {
      const node = graphRef.current.nodes.find((item) => item.id === current.id)
      if (!node) return
      const limit = resizeLimit(node.kind)
      const width = snap(Math.min(limit.maxWidth, Math.max(limit.minWidth, current.width + (event.clientX - current.x) / camera.scale)))
      const height = limit.minHeight === undefined ? undefined : snap(Math.min(limit.maxHeight ?? width, Math.max(limit.minHeight, current.height + (event.clientY - current.y) / camera.scale)))
      publish(replaceNode(graphRef.current, current.id, (item) => ({ ...item, size: { width, height: height ?? item.size?.height ?? current.height } })), current.moved ? 'change' : 'begin')
      current.moved = true
    } else if (current.kind === 'marquee' && viewport.current) {
      const rect = viewport.current.getBoundingClientRect()
      const left = Math.min(current.x, event.clientX) - rect.left
      const top = Math.min(current.y, event.clientY) - rect.top
      setMarquee({ x: left, y: top, width: Math.abs(event.clientX - current.x), height: Math.abs(event.clientY - current.y) })
    }
  }

  const onPointerUp = (event: ReactPointerEvent) => {
    const current = gesture.current
    gesture.current = null
    setDraft(null)
    setMarquee(null)
    if (!current) return
    if (current.kind === 'move' || current.kind === 'resize' || current.kind === 'erase') {
      if (current.moved) publish(graphRef.current, 'end')
    } else if (current.kind === 'draw' && current.points.length > 1) {
      const stroke = { id: current.id, brush: current.brush, color: '#1c1c1e', width: current.width, points: current.points }
      commit(addStroke(graphRef.current, stroke))
    } else if (current.kind === 'connect') {
      const hit = document.elementFromPoint(event.clientX, event.clientY)?.closest('[data-port="in"]') as HTMLElement | null
      const to = hit?.dataset.node
      if (to) {
        const result = addConnection(graph, current.from, to, current.branch)
        if (result.error) setHint(result.error)
        else commit(result.graph)
      } else if (!targetInsideChrome(event) && !scripted) {
        setMenu({ x: event.clientX, y: event.clientY, at: world(event), link: { from: current.from, branch: current.branch } })
      }
    } else if (current.kind === 'marquee') {
      const rect = viewport.current?.getBoundingClientRect()
      if (!rect) return
      const a = worldPoint(camera, { x: current.x, y: current.y }, { x: rect.left, y: rect.top })
      const b = world(event)
      const picked = new Set(current.base)
      for (const node of graph.nodes) {
        const box = boxes.get(node.id)
        if (!box) continue
        const hit = box.x < Math.max(a.x, b.x) && box.x + box.width > Math.min(a.x, b.x) && box.y < Math.max(a.y, b.y) && box.y + box.height > Math.min(a.y, b.y)
        if (hit && !node.locked) picked.add(node.id)
      }
      setSelected(picked)
    }
  }

  function choose(id: string, shift: boolean) {
    const next = new Set(shift ? selected : selected.has(id) ? selected : [id])
    if (shift) next.has(id) ? next.delete(id) : next.add(id)
    setSelected(next)
    setSelectedEdge(null)
    return next
  }

  const onNode = (node: Node) => publish(replaceNode(graphRef.current, node.id, () => node), 'text')
  const place = (kind: 'trigger' | 'prompt' | 'agent' | 'condition' | 'approval' | 'output' | 'text' | 'note' | 'shape', at: Point, link?: { from: string; branch?: Branch | null }) => {
    if (readOnly || scripted) return
    commit(addNode(graph, kind, at, link))
    setMenu(null)
  }

  return (
    <div
      className={`workflow-viewport${chrome ? '' : ' chrome-hidden'}`}
      ref={viewport}
      onContextMenu={onContextMenu}
      tabIndex={0}
      style={grid}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onKeyDown={onKey}
      onKeyUp={onKey}
    >
      <div className="workflow-world" style={{ transform: `translate(${camera.x}px, ${camera.y}px) scale(${camera.scale})` }}>
        <svg className="workflow-edges">
          {graph.strokes.map((stroke) => (
            <path key={stroke.id} d={stroke.points.map((point, index) => `${index ? 'L' : 'M'} ${point.x} ${point.y}`).join(' ')} stroke={stroke.color} strokeWidth={stroke.width} fill="none" strokeLinecap="round" strokeLinejoin="round" />
          ))}
        </svg>
        <WorkflowEdges edges={draws} selected={selectedEdge} draft={draft} />
        <WorkflowNodes
          graph={graph}
          selected={selected}
          steps={steps}
          readOnly={readOnly}
          now={now}
          onEdit={onNode}
          onRunFrom={onRunFrom}
          onRunStep={onRunStep}
          onRetry={onRetry}
          onDecide={onDecide}
          onStopStep={onStopStep}
          onCopy={(id) => { setSelected(new Set([id])); rememberClip(graph.nodes.filter((node) => node.id === id), []) }}
          onLock={(id) => commit(replaceNode(graph, id, (node) => ({ ...node, locked: !node.locked })))}
          onDelete={(id) => commit(deleteSelection(graph, new Set([id]), new Set()))}
          onHeight={(id, height) => {
            if (heights.current.get(id) === height) return
            heights.current.set(id, height)
            setMeasured((value) => value + 1)
            if (readOnly || scripted || gesture.current) return
            queueMicrotask(() => {
              if (gesture.current) return
              const next = spreadOverlaps(graphRef.current, heights.current)
              if (next) commit(next)
            })
          }}
        />
      </div>
      {marquee ? <div className="workflow-marquee" style={{ left: marquee.x, top: marquee.y, width: marquee.width, height: marquee.height }} /> : null}
      {ready && graph.nodes.length === 0 ? (
        <div className="workflow-empty">
          <VStack gap={10} align="center">
            <span className="workflow-empty-mark"><Icon name="bolt" size={20} /></span>
            <Text weight="semibold">{tr('Start with a trigger')}</Text>
            {!readOnly && !scripted ? (
              <Button onClick={() => place('trigger', center())}>
                <Icon name="plus" size={14} />
                <span>{tr('Add trigger')}</span>
              </Button>
            ) : null}
          </VStack>
        </div>
      ) : null}
      {chrome && query !== null ? (
        <div className="workflow-search">
          <SearchField value={query} prompt="Search" onChange={(value) => {
            setQuery(value)
            const text = value.trim().toLowerCase()
            if (!text) return
            setSelected(new Set(graph.nodes.filter((node) => searchText(node, (id) => store.bots.get(id)?.name).includes(text)).map((node) => node.id)))
          }} />
        </div>
      ) : null}
      {hint ? <div className="workflow-banner"><Text size="caption">{hint}</Text></div> : null}
      {chrome && !readOnly ? (
        <WorkflowTools
          tool={tool}
          onTool={setTool}
          searching={query !== null}
          onSearch={() => setQuery((value) => value === null ? '' : null)}
          onAdd={scripted ? undefined : (kind) => place(kind, center())}
        />
      ) : null}
      {chrome ? <WorkflowZoom scale={camera.scale} onZoom={zoom} onFit={fit} minimap={minimap} onMinimap={() => setMinimap((open) => !open)} /> : null}
      {chrome && minimap && size.width > 0 ? <WorkflowMinimap graph={graph} camera={camera} viewport={size} onJump={(next) => { moved.current = true; setCamera(next) }} /> : null}
      {!chrome ? (
        <div className="workflow-chrome-back">
          <Button kind="secondary" onClick={toggleChrome}>
            <Icon name="eye" size={14} />
            <span>{tr('Show interface')}</span>
          </Button>
        </div>
      ) : null}
      <WorkflowAddMenu point={menu ? { x: menu.x, y: menu.y } : null} at={menu?.at} link={menu?.link ? { ...menu.link, at: menu.at } : null} onAdd={place} onClose={() => setMenu(null)} />
      <AnchoredMenu open={canvasMenu !== null} point={canvasMenu} items={canvasItems} onClose={() => setCanvasMenu(null)} />
    </div>
  )
}

type Gesture =
  | { kind: 'pan'; x: number; y: number }
  | { kind: 'move'; ids: Set<string>; origins: Map<string, Point>; x: number; y: number; moved: boolean }
  | { kind: 'marquee'; x: number; y: number; add: boolean; base: Set<string> }
  | { kind: 'connect'; from: string; branch: Branch | null; at: Point }
  | { kind: 'draw'; id: string; brush: 'pen' | 'marker'; width: number; points: Point[]; moved: boolean }
  | { kind: 'erase'; moved: boolean }
  | { kind: 'resize'; id: string; x: number; y: number; width: number; height: number; moved: boolean }

function glide(target: Camera, setCamera: (camera: Camera) => void, cameraRef: { current: Camera }) {
  const from = cameraRef.current
  if (reduceMotion()) {
    setCamera(target)
    return
  }
  const duration = fitDuration(from, target)
  const start = performance.now()
  const tick = (now: number) => {
    const t = (now - start) / duration
    setCamera(t >= 1 ? target : lerpCamera(from, target, t))
    if (t < 1) requestAnimationFrame(tick)
  }
  requestAnimationFrame(tick)
}

function anchorMap(graph: Graph, boxes: Map<string, { x: number; y: number; width: number; height: number }>) {
  const map = new Map<string, { inn: Point; outs: Map<string, Point> }>()
  for (const node of graph.nodes) {
    const box = boxes.get(node.id)
    if (!box) continue
    const outs = new Map<string, Point>()
    if (ports(node.kind)[1].length) {
      if (node.kind === 'condition') {
        outs.set('yes', { x: box.x + box.width, y: box.y + box.height * 0.32 })
        outs.set('no', { x: box.x + box.width, y: box.y + box.height * 0.72 })
      } else outs.set('', { x: box.x + box.width, y: box.y + box.height / 2 })
    }
    map.set(node.id, { inn: { x: box.x, y: box.y + box.height / 2 }, outs })
  }
  return map
}

const SAMPLE = 1.5
const ERASER = 8

function targetInsideChrome(event: { clientX: number; clientY: number }) {
  return !!document.elementFromPoint(event.clientX, event.clientY)?.closest('.workflow-tools, .workflow-zoom, .workflow-minimap')
}
