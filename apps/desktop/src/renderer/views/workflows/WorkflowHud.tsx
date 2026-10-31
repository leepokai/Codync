import { useRef, useState } from 'react'
import { Button, DropdownMenu, IconButton, Switch } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { AnchoredMenu, type MenuItem } from '../../components/Overlay'
import { HStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { WorkflowTip } from './WorkflowTip'
import { progressOf, runClock, runWhen, type Graph, type RunSummary, type WorkflowStep, type WorkflowSummary, type WorkflowVersion } from './workflow'

export function WorkflowHud({ title, selectedId, workflows, enabled, graph, steps, live, runs, hasMore, followLive, viewing, versions, previewLabel, pinned, notice, scripted, canUndo, canRedo, onUndo, onRedo, onOpen, onCreate, onRename, onDuplicate, onDelete, onPin, onEnabled, onRun, onStop, onLatest, onRunOpen, onOlder, onLoadVersions, onPreview, onRestore, onScript, onShowHistory }: {
  title: string
  selectedId: string | null
  workflows: WorkflowSummary[]
  enabled: boolean
  graph: Graph | null
  steps: WorkflowStep[]
  live: boolean
  runs: RunSummary[]
  hasMore: boolean
  followLive: boolean
  viewing: RunSummary | null
  versions: WorkflowVersion[]
  previewLabel: string | null
  pinned: boolean
  notice: string | null
  scripted: boolean
  canUndo: boolean
  canRedo: boolean
  onUndo: () => void
  onRedo: () => void
  onOpen: (id: string) => void
  onCreate: () => void
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
  onRestore: () => void
  onScript: () => void
  onShowHistory: () => void
}) {
  const historyButton = useRef<HTMLButtonElement>(null)
  const titleAnchor = useRef<HTMLSpanElement>(null)
  const [historyOpen, setHistoryOpen] = useState(false)
  const [versionsOpen, setVersionsOpen] = useState(false)
  const tr = useTr()
  const progress = graph ? progressOf(graph, steps) : null
  const menu = (): MenuItem[] => [
    ...workflows.map((item) => ({ title: item.title, verbatim: true, selected: item.id === selectedId, action: () => onOpen(item.id) })),
    { title: 'New workflow', icon: 'plus', divider: workflows.length > 0, action: onCreate },
    { title: 'Versions', divider: true, action: () => { onLoadVersions(); setVersionsOpen(true) } },
    { title: 'Rename', action: onRename },
    { title: 'Duplicate', action: onDuplicate },
    { title: pinned ? 'Unpin' : 'Pin', action: onPin },
    { title: 'Delete workflow', destructive: true, action: onDelete },
  ]
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
  const versionItems = (): MenuItem[] => versions.map((version) => ({
    title: `Version ${version.revision}`,
    detail: versionDetail(version, tr),
    action: () => onPreview(version),
  }))
  return (
    <div className="workflow-hud">
      <HStack gap={8}>
        {title ? <IconButton title="Undo" icon="arrow.uturn.backward" disabled={!canUndo} onClick={onUndo} /> : null}
        {title ? <IconButton title="Redo" icon="arrow.uturn.forward" disabled={!canRedo} onClick={onRedo} /> : null}
        <span ref={titleAnchor} className="workflow-title-wrap">
          <DropdownMenu className="workflow-title" title={title || 'Workflows'} items={menu}>
            <Text weight="semibold" truncate>{title || 'Workflows'}</Text>
          </DropdownMenu>
        </span>
        {title ? <Switch className="workflow-enabled" on={enabled} label="Enabled" onChange={onEnabled} /> : null}
        {title ? (
          <WorkflowTip label="Script" below>
            <IconButton title="Script" systemTip={false} icon="chevron.left.forwardslash.chevron.right" onClick={onScript} />
          </WorkflowTip>
        ) : null}
        {progress && progress.total > 0 && !previewLabel && (live || (viewing && !followLive)) ? <Text size="caption" tone="secondary" tabular>{progress.done}/{progress.total}</Text> : null}
        <AnchoredMenu open={versionsOpen} onClose={() => setVersionsOpen(false)} anchor={titleAnchor} items={versionItems} />
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
          <button ref={historyButton} type="button" className="workflow-history" aria-expanded={historyOpen} onClick={() => setHistoryOpen((open) => { if (!open) onShowHistory(); return !open })}>
            <Icon name="clock" size={14} />
            <span>{tr('History')}</span>
            <span className="workflow-history-count">{runs.length}</span>
          </button>
        ) : null}
        <AnchoredMenu open={historyOpen} onClose={() => setHistoryOpen(false)} anchor={historyButton} items={history} />
        {title && !previewLabel ? (
          <Button className="workflow-run-now" onClick={live ? onStop : onRun}>
            {live ? tr('Stop') : <><Icon name="play.fill" size={12} /><span>{tr('Run workflow')}</span></>}
          </Button>
        ) : null}
      </HStack>
      {notice ? (
        <HStack gap={8} className="workflow-banner">
          <Text size="caption" tone="secondary">{tr(notice)}</Text>
        </HStack>
      ) : null}
      {scripted && !previewLabel ? (
        <HStack gap={8} className="workflow-banner">
          <Text size="caption" grow>This workflow decides its steps when it runs.</Text>
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

function versionDetail(version: WorkflowVersion, tr: (text: string) => string) {
  const when = runWhenLabel(version.createdAt, tr)
  if (version.kind === 'restore') return `${tr('Before restore')} · ${when}`
  if (version.kind === 'run') return `${tr('Run started')} · ${when}`
  return when
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
