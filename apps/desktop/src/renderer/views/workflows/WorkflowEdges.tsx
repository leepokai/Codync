import { edgeMidpoint, edgePath } from './edges'
import { loopEdges, type Graph, type Point, type WorkflowStep } from './workflow'
import { Text } from '../../components/Text'

export interface EdgeDraw {
  id: string
  from: Point
  to: Point
  status: 'running' | 'done' | 'idle'
  loop: boolean
}

export function edgeDraws(graph: Graph, anchors: Map<string, { inn: Point; outs: Map<string, Point> }>, steps: Map<string, WorkflowStep>): EdgeDraw[] {
  const loops = loopEdges(graph)
  return graph.edges.flatMap((edge) => {
    const from = anchors.get(edge.from)?.outs.get(edge.branch ?? '')
    const to = anchors.get(edge.to)?.inn
    if (!from || !to) return []
    const step = steps.get(edge.from)
    const status = step?.status === 'running' || step?.status === 'approval' ? 'running' : step?.status === 'done' ? 'done' : 'idle'
    return [{ id: edge.id, from, to, status, loop: loops.has(edge.id) }]
  })
}

export function WorkflowEdges({ edges, selected, draft }: { edges: EdgeDraw[]; selected: string | null; draft: string | null }) {
  return (
    <>
      <svg className="workflow-edges">
        {edges.map((edge) => (
          <path key={edge.id} className={`workflow-edge ${edge.status} ${selected === edge.id ? 'selected' : ''}`} d={edgePath(edge.from, edge.to)} data-edge={edge.id} />
        ))}
        {draft ? <path className="workflow-edge running" d={draft} /> : null}
        {edges.map((edge) => <path key={`${edge.id}-hit`} className="workflow-edge hit" d={edgePath(edge.from, edge.to)} data-edge={edge.id} />)}
      </svg>
      {edges.filter((edge) => edge.loop).map((edge) => {
        const mid = edgeMidpoint(edge.from, edge.to)
        return (
          <div key={`${edge.id}-loop`} className="workflow-loop" style={{ transform: `translate(${mid.x}px, ${mid.y}px)` }}>
            <Text size="caption" tone="secondary">Loop</Text>
          </div>
        )
      })}
    </>
  )
}
