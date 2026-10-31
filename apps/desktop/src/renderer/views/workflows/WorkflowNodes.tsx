import { Button, IconButton } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { WorkflowFields } from './WorkflowFields'
import { fieldString, ports, type Branch, type Graph, type Node, type WorkflowStep } from './workflow'

const KIND: Record<string, { title: string; icon: string }> = {
  trigger: { title: 'Trigger', icon: 'bolt' },
  prompt: { title: 'Prompt', icon: 'text.bubble' },
  agent: { title: 'Assistant', icon: 'person.fill' },
  condition: { title: 'Condition', icon: 'arrow.triangle.branch' },
  approval: { title: 'Approval', icon: 'person.crop.circle.badge.checkmark' },
  output: { title: 'Output', icon: 'paperplane' },
  text: { title: 'Text', icon: 'doc.plaintext' },
  note: { title: 'Note', icon: 'doc.text' },
  shape: { title: 'Shape', icon: 'circle' },
}

export function WorkflowNodes({ graph, selected, steps, readOnly, now, onEdit, onRunFrom, onRunStep, onRetry, onDecide, onStopStep, onCopy, onLock, onDelete, onHeight }: {
  graph: Graph
  selected: Set<string>
  steps: Map<string, WorkflowStep>
  readOnly: boolean
  now: number
  onEdit: (node: Node) => void
  onRunFrom: (nodeId: string) => void
  onRunStep: (nodeId: string) => void
  onRetry: (stepId: string) => void
  onDecide: (stepId: string, approved: boolean) => void
  onStopStep: (stepId: string) => void
  onCopy: (id: string) => void
  onLock: (id: string) => void
  onDelete: (id: string) => void
  onHeight: (id: string, height: number) => void
}) {
  const tr = useTr()
  return (
    <>
      {graph.nodes.map((node) => {
        const tone = node.kind === 'note' ? fieldString(node.fields, 'tone') : node.kind === 'shape' ? fieldString(node.fields, 'fill') : ''
        const step = steps.get(node.id)
        const alone = selected.size === 1 && selected.has(node.id)
        const kind = KIND[node.kind]
        const fed = graph.edges.some((edge) => edge.to === node.id)
        return (
          <article
            key={node.id}
            className={`workflow-node kind-${node.kind} ${selected.has(node.id) ? 'selected' : ''} ${tone ? `tone-${tone}` : ''}`}
            data-node={node.id}
            style={{ transform: `translate(${node.at.x}px, ${node.at.y}px)`, width: node.size?.width, height: node.size?.height }}
            ref={(el) => { if (el && el.offsetHeight) onHeight(node.id, el.offsetHeight) }}
          >
            {alone && !readOnly ? (
              <div className="workflow-toolbar">
                <IconButton title="Copy" icon="doc.on.doc" onClick={() => onCopy(node.id)} />
                <IconButton title={node.locked ? 'Unlock' : 'Lock'} icon="lock.shield" selected={node.locked} onClick={() => onLock(node.id)} />
                <IconButton title="Delete" icon="trash" disabled={node.locked} onClick={() => onDelete(node.id)} />
              </div>
            ) : null}
            <div className="workflow-label">
              <Icon name={kind?.icon ?? 'circle'} size={12} />
              <Text size="caption" weight="semibold">{tr(kind?.title ?? node.kind)}</Text>
            </div>
            <WorkflowFields node={node} readOnly={readOnly || node.locked} onChange={onEdit} />
            <StepResult kind={node.kind} step={step} now={now} onRetry={onRetry} onDecide={onDecide} onStop={onStopStep} />
            {node.kind === 'trigger' && !readOnly ? (
              <Button className="workflow-run" onClick={() => onRunFrom(node.id)}>
                <span>{tr('Run workflow')}</span>
                <Icon name="play.fill" size={12} />
              </Button>
            ) : null}
            {node.kind === 'agent' && !readOnly ? (
              <Button className="workflow-run" disabled={!fieldString(node.fields, 'botId') || !fieldString(node.fields, 'task').trim()} onClick={() => onRunStep(node.id)}>
                <span>{tr('Run step')}</span>
                <Icon name="play.fill" size={12} />
              </Button>
            ) : null}
            {node.kind === 'output' && !readOnly ? (
              <Button className="workflow-run" disabled={!fed} onClick={() => onRunStep(node.id)}>
                <span>{tr('Run')}</span>
                <Icon name="play.fill" size={12} />
              </Button>
            ) : null}
            <Ports node={node} />
            {!readOnly && !node.locked && (node.kind === 'trigger' || node.kind === 'condition' || node.kind === 'note') ? <div className="workflow-resize" data-resize={node.id} /> : null}
          </article>
        )
      })}
    </>
  )
}

function Ports({ node }: { node: Node }) {
  const [ins, outs] = ports(node.kind)
  return (
    <>
      {ins.map((type) => <button key={`in-${type}`} type="button" className="workflow-port in" data-port="in" data-node={node.id} aria-label="Input"><Text size="caption">T</Text></button>)}
      {outs.map((type) => node.kind === 'condition'
        ? (['yes', 'no'] as Branch[]).map((branch) => (
          <button key={branch} type="button" className={`workflow-port out ${branch}`} data-port="out" data-node={node.id} data-branch={branch} aria-label={branch === 'yes' ? 'Yes' : 'No'}>
            <Text size="caption">{branch === 'yes' ? 'Y' : 'N'}</Text>
          </button>
        ))
        : <button key={`out-${type}`} type="button" className="workflow-port out" data-port="out" data-node={node.id} aria-label="Output"><Text size="caption">T</Text></button>)}
    </>
  )
}

function StepResult({ kind, step, now, onRetry, onDecide, onStop }: {
  kind: string
  step: WorkflowStep | undefined
  now: number
  onRetry: (stepId: string) => void
  onDecide: (stepId: string, approved: boolean) => void
  onStop: (stepId: string) => void
}) {
  const tr = useTr()
  if (!step) return null
  if (step.status === 'running') {
    const seconds = step.startedAt ? Math.max(0, Math.round((now - step.startedAt) / 1000)) : 0
    return (
      <HStack gap={8} className="workflow-result">
        <Text size="caption" tone="secondary" grow>{tr('Waiting {seconds}s').replaceAll('{seconds}', String(seconds))}</Text>
        <Button kind="secondary" onClick={() => onStop(step.id)}>Stop</Button>
      </HStack>
    )
  }
  if (step.status === 'failed') {
    return (
      <VStack gap={4} className="workflow-result">
        <Text size="caption" tone="danger">{step.error || 'Failed'}</Text>
        <Button kind="secondary" onClick={() => onRetry(step.id)}>Retry</Button>
      </VStack>
    )
  }
  if (step.status === 'approval') {
    return (
      <HStack gap={8} className="workflow-result">
        <Button onClick={() => onDecide(step.id, true)}>Approve</Button>
        <Button kind="secondary" onClick={() => onDecide(step.id, false)}>Reject</Button>
      </HStack>
    )
  }
  if (step.status === 'skipped') return <Text size="caption" tone="secondary">Skipped</Text>
  if (step.status === 'stopped') return <Text size="caption" tone="secondary">Stopped</Text>
  if (step.output && kind === 'output') {
    return (
      <div className="workflow-done">
        <Icon name="checkmark.circle.fill" size={14} />
        <Text size="caption" tone="added" lines={4}>{step.output}</Text>
      </div>
    )
  }
  if (step.output) return <Text size="caption" lines={4}>{step.output}</Text>
  return null
}
