import { useTr } from '../../lib/use-language'
import { boundsOf, minimapPoint, viewBounds, type Camera } from './camera'
import { nodeBox, type Graph } from './workflow'

const SIZE = { width: 160, height: 100 }

export function WorkflowMinimap({ graph, camera, viewport, onJump }: {
  graph: Graph
  camera: Camera
  viewport: { width: number; height: number }
  onJump: (camera: Camera) => void
}) {
  const tr = useTr()
  const rects = graph.nodes.map((node) => nodeBox(node))
  const bounds = boundsOf(rects)
  const view = viewBounds(camera, viewport)
  const frame = minimapPoint(view, bounds, SIZE)
  const frameSize = minimapPoint({ x: view.x + view.width, y: view.y + view.height }, bounds, SIZE)
  return (
    <button
      type="button"
      className="workflow-minimap"
      aria-label={tr('Minimap')}
      onClick={(event) => {
        const rect = event.currentTarget.getBoundingClientRect()
        const worldX = bounds.x + ((event.clientX - rect.left) / SIZE.width) * bounds.width
        const worldY = bounds.y + ((event.clientY - rect.top) / SIZE.height) * bounds.height
        onJump({ ...camera, x: viewport.width / 2 - worldX * camera.scale, y: viewport.height / 2 - worldY * camera.scale })
      }}
    >
      {rects.map((rect) => {
        const at = minimapPoint(rect, bounds, SIZE)
        const far = minimapPoint({ x: rect.x + rect.width, y: rect.y + rect.height }, bounds, SIZE)
        return <span key={`${rect.x}-${rect.y}`} className="workflow-minimap-node" style={{ left: at.x, top: at.y, width: Math.max(far.x - at.x, 2), height: Math.max(far.y - at.y, 2) }} />
      })}
      <span className="workflow-minimap-view" style={{ left: frame.x, top: frame.y, width: Math.max(frameSize.x - frame.x, 8), height: Math.max(frameSize.y - frame.y, 8) }} />
    </button>
  )
}
