import { useRef, useState } from 'react'
import { IconButton } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { useTr } from '../../lib/use-language'
import { WorkflowAddMenu } from './WorkflowAddMenu'
import { chord, WorkflowTip } from './WorkflowTip'
import type { Tool } from './keys'
import type { NodeKind } from './workflow'

const DRAWING: Tool[] = ['pen', 'marker', 'eraser']

export function WorkflowTools({ tool, onTool, searching, onSearch, onAdd }: {
  tool: Tool
  onTool: (tool: Tool) => void
  searching: boolean
  onSearch: () => void
  onAdd?: (kind: NodeKind) => void
}) {
  const tr = useTr()
  const addRef = useRef<HTMLButtonElement>(null)
  const [addOpen, setAddOpen] = useState(false)
  const drawing = DRAWING.includes(tool)
  return (
    <>
      {drawing ? (
        <div className="workflow-tools workflow-tools-sub" role="toolbar" aria-label={tr('Draw')}>
          <WorkflowTip label="Pen" keys="P"><IconButton title="Pen" systemTip={false} icon="pencil" selected={tool === 'pen'} onClick={() => onTool('pen')} /></WorkflowTip>
          <WorkflowTip label="Marker" keys="M"><IconButton title="Marker" systemTip={false} icon="square.and.pencil" selected={tool === 'marker'} onClick={() => onTool('marker')} /></WorkflowTip>
          <WorkflowTip label="Eraser" keys="E"><IconButton title="Eraser" systemTip={false} icon="eraser" selected={tool === 'eraser'} onClick={() => onTool('eraser')} /></WorkflowTip>
        </div>
      ) : null}
      <div className="workflow-tools" role="toolbar" aria-label={tr('Tools')}>
        <WorkflowTip label="Select" keys="V"><IconButton title="Select" systemTip={false} icon="cursorarrow" selected={tool === 'select'} onClick={() => onTool('select')} /></WorkflowTip>
        <WorkflowTip label="Pan the canvas" keys="H"><IconButton title="Pan the canvas" systemTip={false} icon="hand.raised" selected={tool === 'pan'} onClick={() => onTool('pan')} /></WorkflowTip>
        <span className="workflow-tools-sep" />
        <WorkflowTip label="Pen" keys="P"><IconButton title="Pen" systemTip={false} icon="pencil" selected={drawing} onClick={() => onTool('pen')} /></WorkflowTip>
        <WorkflowTip label="Note" keys="N"><IconButton title="Note" systemTip={false} icon="doc.text" selected={tool === 'note'} onClick={() => onTool('note')} /></WorkflowTip>
        <WorkflowTip label="Shape" keys="R"><IconButton title="Shape" systemTip={false} icon="circle" selected={tool === 'shape'} onClick={() => onTool('shape')} /></WorkflowTip>
        <WorkflowTip label="Text" keys="T"><IconButton title="Text" systemTip={false} icon="doc.plaintext" selected={tool === 'text'} onClick={() => onTool('text')} /></WorkflowTip>
        <span className="workflow-tools-sep" />
        <WorkflowTip label="Search" keys={chord('mod', 'F')}><IconButton title="Search" systemTip={false} icon="magnifyingglass" selected={searching} onClick={onSearch} /></WorkflowTip>
        {onAdd ? (
          <button
            ref={addRef}
            type="button"
            className="styled-button primary workflow-add"
            aria-expanded={addOpen}
            onClick={() => setAddOpen((open) => !open)}
          >
            <Icon name="plus" size={14} />
            <span>{tr('Add node')}</span>
          </button>
        ) : null}
      </div>
      {onAdd ? (
        <WorkflowAddMenu
          open={addOpen}
          anchor={addRef}
          onAdd={(kind) => { setAddOpen(false); onAdd(kind) }}
          onClose={() => setAddOpen(false)}
        />
      ) : null}
    </>
  )
}
