import { useRef, useState } from 'react'
import { Button, IconButton, Switch } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { AnchoredMenu, type MenuItem } from '../../components/Overlay'
import { HStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { WorkflowTip } from './WorkflowTip'
import { WorkflowTitleMenu } from './WorkflowTitleMenu'
import { progressOf, runClock, runWhen, type Graph, type RunSummary, type WorkflowStep, type WorkflowVersion } from './workflow'

export function WorkflowHud({ title, enabled, graph, steps, live, runs, hasMore, followLive, viewing, versions, previewId, previewLabel, pinned, notice, scripted, canUndo, canRedo, onUndo, onRedo, onRename, onDuplicate, onDelete, onPin, onEnabled, onRun, onStop, onLatest, onRunOpen, onOlder, onLoadVersions, onPreview, onShowCurrent, onRestore, onScript, onShowHistory }: {
  title: string
  enabled: boolean
  graph: Graph | null
  steps: WorkflowStep[]
  live: boolean
  runs: RunSummary[]
  hasMore: boolean
  followLive: boolean
  viewing: RunSummary | null
  versions: WorkflowVersion[]
  previewId: string | null
  previewLabel: string | null
  pinned: boolean
  notice: string | null
  scripted: boolean
  canUndo: boolean
  canRedo: boolean
  onUndo: () => void
  onRedo: () => void
  onRename: () => void
  onDuplicate: () => void
  onDelete: () => void
  onPin: () => void
  onEnabled: (enabled: boolean) => void
  onRun: () => void
  onStop: () => void
  onLatest: () => void
  onRunOpen: (id: string) => void
  onOlder: () => void
  onLoadVersions: () => void
  onPreview: (version: WorkflowVersion) => void
  onShowCurrent: () => void
  onRestore: () => void
  onScript: () => void
  onShowHistory: () => void
}) {
  const historyButton = useRef<HTMLButtonElement>(null)
  const [historyOpen, setHistoryOpen] = useState(false)
  const tr = useTr()
  const progress = graph ? progressOf(graph, steps) : null
  const history = (): MenuItem[] => [
    { title: 'Latest results', selected: followLive, action: onLatest },
    ...runs.map((run) => ({
      title: tr('Run {number}').replaceAll('{number}', String(run.number)),
      verbatim: true,
      detail: runDetail(run, tr),
      detailBeside: true,
      selected: viewing?.id === run.id,
      action: () => onRunOpen(run.id),
    })),
    ...(hasMore ? [{ title: 'Show older runs', action: onOlder }] : []),
  ]
  return (
    <div className="workflow-hud">
      <HStack gap={8}>
        {title ? <IconButton title="Undo" icon="arrow.uturn.backward" disabled={!canUndo} onClick={onUndo} /> : null}
        {title ? <IconButton title="Redo" icon="arrow.uturn.forward" disabled={!canRedo} onClick={onRedo} /> : null}
        {title ? (
          <span className="workflow-title-wrap">
            <WorkflowTitleMenu
              title={title}
              versions={versions}
              previewId={previewId}
              pinned={pinned}
              onRename={onRename}
              onDuplicate={onDuplicate}
              onDelete={onDelete}
              onPin={onPin}
              onLoadVersions={onLoadVersions}
              onPreview={onPreview}
              onShowCurrent={onShowCurrent}
            />
          </span>
        ) : null}
        {title ? <Switch className="workflow-enabled" on={enabled} label="Enabled" onChange={onEnabled} /> : null}
        {title ? (
          <WorkflowTip label="Script" below>
            <IconButton className="workflow-script-button" title="Script" systemTip={false} icon="chevron.left.forwardslash.chevron.right" size={34} iconSize={22} onClick={onScript} />
          </WorkflowTip>
        ) : null}
        {progress && progress.total > 0 && !previewLabel && (live || (viewing && !followLive)) ? <Text size="caption" tone="secondary" tabular>{progress.done}/{progress.total}</Text> : null}
        <HStack grow />
        {viewing && !followLive ? (
          <span className="workflow-viewing">
            <Icon name="clock" size={14} />
            <span>{tr('Run {number}').replaceAll('{number}', String(viewing.number))}</span>
            <span className="workflow-viewing-time">{runWhenLabel(viewing.startedAt, tr)}</span>
            <WorkflowTip label="Latest results" below>
              <IconButton title="Latest results" systemTip={false} icon="xmark" size={22} onClick={onLatest} />
            </WorkflowTip>
          </span>
        ) : null}
        {title ? (
          <WorkflowTip label="History" below>
            <button ref={historyButton} type="button" className="workflow-history" aria-label={tr('History')} aria-expanded={historyOpen} onClick={() => setHistoryOpen((open) => { if (!open) onShowHistory(); return !open })}>
              <Icon name="clock" size={14} />
              <span className="workflow-hud-label">{tr('History')}</span>
              <span className="workflow-history-count">{runs.length}</span>
            </button>
          </WorkflowTip>
        ) : null}
        <AnchoredMenu open={historyOpen} onClose={() => setHistoryOpen(false)} anchor={historyButton} items={history} />        {title && !previewLabel ? (
          <WorkflowTip label={live ? 'Stop' : 'Run workflow'} below>
            <Button className="workflow-run-now" label={live ? 'Stop' : 'Run workflow'} onClick={live ? onStop : onRun}>
              {live ? tr('Stop') : <><Icon name="play.fill" size={12} /><span className="workflow-hud-label">{tr('Run workflow')}</span></>}
            </Button>
          </WorkflowTip>
        ) : null}
      </HStack>
      {notice ? (
        <HStack gap={8} className="workflow-banner">
          <Text size="caption" tone="secondary">{tr(notice)}</Text>
        </HStack>
      ) : null}
      {scripted && !previewLabel ? (
        <HStack gap={8} className="workflow-banner">
          <Text size="caption" grow>{tr('This workflow decides its steps when it runs.')}</Text>
        </HStack>
      ) : null}
      {previewLabel ? (
        <HStack gap={8} className="workflow-banner">
          <Text size="caption" grow>{previewLabel}</Text>
          <Button onClick={onRestore}>Make current</Button>
        </HStack>
      ) : null}
    </div>
  )
}

function runWhenLabel(startedAt: number, tr: (text: string) => string) {
  const started = new Date(startedAt)
  const time = started.toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit', hourCycle: 'h23' })
  return started.toDateString() === new Date().toDateString() ? tr('Today {time}').replaceAll('{time}', time) : runWhen(startedAt)
}

function runDetail(run: RunSummary, tr: (text: string) => string) {
  const seconds = Math.max(0, (run.endedAt ?? Date.now()) - run.startedAt) / 1000
  const duration = seconds < 60 ? tr('{seconds} s').replaceAll('{seconds}', seconds.toFixed(3)) : runClock(run.startedAt, run.endedAt)
  return `${runWhenLabel(run.startedAt, tr)} · ${duration}`
}
