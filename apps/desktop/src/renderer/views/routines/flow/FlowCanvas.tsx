import { useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type PointerEvent } from 'react'
import type { Bot } from '@shared/models'
import { IconButton } from '../../../components/Controls'
import { useTr } from '../../../lib/use-language'
import {
  NODE_HEIGHT, NODE_WIDTH, edgePath, fitView, flowEdges, isUnconnected, latestStep, layoutNodes, nodeAt, stepOf, zoomAt,
  type FlowCanvas, type FlowNode, type View,
} from '../../../lib/routine-flow.ts'
import { FlowNodeCard } from './FlowNodeCard'

export type FlowSelection = { kind: 'node'; key: string } | { kind: 'edge'; id: string } | null

interface Props {
  canvas: FlowCanvas
  bots: ReadonlyMap<string, Bot>
  selection: FlowSelection
  disabled: boolean
  onSelect: (selection: FlowSelection) => void
  onMove: (nodeKey: string, x: number, y: number) => void
  /** A bot's connection was dropped on another bot. */
  onConnect: (fromBotId: string, toBotId: string, at: { x: number; y: number }) => void
}

const NUDGE = 20
const DRAG_SLOP = 3

/** The canvas: routine and bot nodes, the edges between them, pan (drag the background), zoom (wheel and buttons). */
export function FlowCanvasView({ canvas, bots, selection, disabled, onSelect, onMove, onConnect }: Props) {
  const tr = useTr()
  const nodes = useMemo(() => layoutNodes(canvas), [canvas])
  const edges = useMemo(() => flowEdges(canvas), [canvas])
  const [view, setView] = useState<View>({ x: 0, y: 0, scale: 1 })
  const [live, setLive] = useState<{ key: string; x: number; y: number } | null>(null)
  const [wire, setWire] = useState<{ from: string; x: number; y: number } | null>(null)
  const viewport = useRef<HTMLDivElement>(null)
  const fitted = useRef(false)
  const viewRef = useRef(view)
  viewRef.current = view

  const size = () => viewport.current?.getBoundingClientRect() ?? new DOMRect(0, 0, 800, 500)
  useLayoutEffect(() => {
    if (fitted.current || nodes.length === 0) return
    fitted.current = true
    const { width, height } = size()
    setView(fitView(nodes, width, height))
  }, [nodes])

  // The wheel zooms around the pointer; it must not scroll the page behind, so it can't be a passive React handler.
  useEffect(() => {
    const element = viewport.current
    if (!element) return
    const wheel = (event: WheelEvent) => {
      event.preventDefault()
      const box = element.getBoundingClientRect()
      setView((v) => zoomAt(v, Math.exp(-event.deltaY * 0.0015), event.clientX - box.left, event.clientY - box.top))
    }
    element.addEventListener('wheel', wheel, { passive: false })
    return () => element.removeEventListener('wheel', wheel)
  }, [])

  const toCanvas = (clientX: number, clientY: number) => {
    const box = size()
    const v = viewRef.current
    return { x: (clientX - box.left - v.x) / v.scale, y: (clientY - box.top - v.y) / v.scale }
  }

  /** Follows the pointer until it lifts. */
  const track = (move: (event: globalThis.PointerEvent) => void, done: (event: globalThis.PointerEvent) => void) => {
    const up = (event: globalThis.PointerEvent) => {
      window.removeEventListener('pointermove', move)
      window.removeEventListener('pointerup', up)
      done(event)
    }
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', up)
  }

  const startPan = (event: PointerEvent) => {
    if (event.target !== event.currentTarget && !(event.target as Element).classList.contains('flow-layer')) return
    onSelect(null)
    let last = { x: event.clientX, y: event.clientY }
    track(
      (e) => {
        const dx = e.clientX - last.x
        const dy = e.clientY - last.y
        last = { x: e.clientX, y: e.clientY }
        setView((v) => ({ ...v, x: v.x + dx, y: v.y + dy }))
      },
      () => {},
    )
  }

  const startDrag = (node: FlowNode, event: PointerEvent) => {
    if (event.button !== 0) return
    const origin = toCanvas(event.clientX, event.clientY)
    const grab = { x: origin.x - node.x, y: origin.y - node.y }
    let moved = false
    let at = { x: node.x, y: node.y }
    track(
      (e) => {
        const p = toCanvas(e.clientX, e.clientY)
        if (!moved && Math.hypot(p.x - origin.x, p.y - origin.y) * viewRef.current.scale < DRAG_SLOP) return
        moved = true
        at = { x: p.x - grab.x, y: p.y - grab.y }
        setLive({ key: node.key, ...at })
      },
      () => {
        setLive(null)
        if (moved) onMove(node.key, at.x, at.y)
      },
    )
  }

  const startWire = (node: FlowNode, event: PointerEvent) => {
    const start = toCanvas(event.clientX, event.clientY)
    setWire({ from: node.id, ...start })
    track(
      (e) => setWire({ from: node.id, ...toCanvas(e.clientX, e.clientY) }),
      (e) => {
        setWire(null)
        const p = toCanvas(e.clientX, e.clientY)
        const target = nodeAt(nodes, p.x, p.y)
        if (target && target.kind === 'bot' && target.id !== node.id) onConnect(node.id, target.id, { x: e.clientX, y: e.clientY })
      },
    )
  }

  const keyOnNode = (node: FlowNode, event: KeyboardEvent) => {
    const step = event.shiftKey ? NUDGE * 5 : NUDGE
    const delta: Record<string, [number, number]> = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] }
    const d = delta[event.key]
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault()
      onSelect({ kind: 'node', key: node.key })
    } else if (d && !disabled) {
      event.preventDefault()
      onMove(node.key, node.x + d[0], node.y + d[1])
    }
  }

  const placed = (node: FlowNode) => (live && live.key === node.key ? { ...node, x: live.x, y: live.y } : node)
  const byKey = new Map(nodes.map((n) => [n.key, placed(n)]))
  const wireFrom = wire ? [...byKey.values()].find((n) => n.kind === 'bot' && n.id === wire.from) : null
  const hovered = wire ? nodeAt([...byKey.values()], wire.x, wire.y) : null
  const zoom = (factor: number) => {
    const box = size()
    setView((v) => zoomAt(v, factor, box.width / 2, box.height / 2))
  }

  return (
    <div className="flow-viewport" ref={viewport} onPointerDown={startPan} role="application" aria-label={tr('Routine flow canvas')}>
      <div className="flow-layer" style={{ transform: `translate(${view.x}px, ${view.y}px) scale(${view.scale})` }}>
        <svg className="flow-edges" width="1" height="1" aria-hidden="true">
          {edges.map((edge) => {
            const from = byKey.get(edge.from)
            const to = byKey.get(edge.to)
            if (!from || !to) return null
            const d = edgePath(from, to)
            const isSelected = selection?.kind === 'edge' && selection.id === edge.linkId
            return (
              <g key={edge.id}>
                <path className="flow-edge" data-kind={edge.linkId ? 'link' : 'start'} data-selected={isSelected} d={d} />
                {edge.linkId ? (
                  <path className="flow-edge-hit" d={d} onPointerDown={(e) => { e.stopPropagation(); onSelect({ kind: 'edge', id: edge.linkId! }) }} />
                ) : null}
              </g>
            )
          })}
          {wire && wireFrom ? (
            <path className="flow-edge" data-kind="wire" d={`M ${wireFrom.x + NODE_WIDTH} ${wireFrom.y + NODE_HEIGHT / 2} L ${wire.x} ${wire.y}`} />
          ) : null}
        </svg>
        {nodes.map((raw) => {
          const node = placed(raw)
          const card = raw.kind === 'routine' ? canvas.routines.find((c) => c.routine.id === raw.id) : undefined
          const step = raw.kind === 'bot' ? latestStep(canvas, raw.id) : null
          return (
            <FlowNodeCard
              key={raw.key}
              node={raw}
              x={node.x}
              y={node.y}
              bot={bots.get(raw.id)}
              routine={card}
              step={card ? stepOf(card, card.routine.botId) : step}
              unconnected={card ? isUnconnected(canvas, card.routine.id) : false}
              selected={selection?.kind === 'node' && selection.key === raw.key}
              connectTarget={hovered?.key === raw.key && raw.kind === 'bot' && raw.id !== wire?.from}
              disabled={disabled}
              onSelect={() => onSelect({ kind: 'node', key: raw.key })}
              onDragStart={(e) => startDrag(raw, e)}
              onPortStart={(e) => startWire(raw, e)}
              onKey={(e) => keyOnNode(raw, e)}
            />
          )
        })}
      </div>
      <div className="flow-zoom">
        <IconButton title={tr('Zoom out')} icon="minus" onClick={() => zoom(1 / 1.25)} />
        <IconButton title={tr('Zoom in')} icon="plus" onClick={() => zoom(1.25)} />
        <IconButton title={tr('Fit to view')} icon="arrow.up.left.and.arrow.down.right" onClick={() => setView(fitView(nodes, size().width, size().height))} />
      </div>
    </div>
  )
}
