import { IconButton } from '../../components/Controls'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { chord, WorkflowTip } from './WorkflowTip'

export function WorkflowZoom({ scale, onZoom, onFit, minimap, onMinimap }: {
  scale: number
  onZoom: (direction: 1 | -1 | 0) => void
  onFit: () => void
  minimap: boolean
  onMinimap: () => void
}) {
  const tr = useTr()
  return (
    <div className="workflow-zoom">
      <WorkflowTip label="Zoom out" keys={chord('mod', '−')}>
        <IconButton title="Zoom out" systemTip={false} icon="minus.circle" onClick={() => onZoom(-1)} />
      </WorkflowTip>
      <WorkflowTip label="Actual size" keys={chord('mod', '0')}>
        <button type="button" className="workflow-title" onClick={() => onZoom(0)} aria-label={tr('Actual size')}>
          <Text size="caption">{Math.round(scale * 100)}%</Text>
        </button>
      </WorkflowTip>
      <WorkflowTip label="Zoom in" keys={chord('mod', '+')}>
        <IconButton title="Zoom in" systemTip={false} icon="plus" onClick={() => onZoom(1)} />
      </WorkflowTip>
      <span className="workflow-tools-sep" />
      <WorkflowTip label="Zoom to fit" keys={chord('⇧', '1')}>
        <IconButton title="Zoom to fit" systemTip={false} icon="viewfinder" onClick={onFit} />
      </WorkflowTip>
      <WorkflowTip label="Minimap">
        <IconButton title="Minimap" systemTip={false} icon="map" selected={minimap} onClick={onMinimap} />
      </WorkflowTip>
    </div>
  )
}
