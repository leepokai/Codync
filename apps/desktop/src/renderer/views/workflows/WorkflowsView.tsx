import { useEffect, useRef, useState } from 'react'
import type { Screen } from '@shared/analytics'
import { Button } from '../../components/Controls'
import { Dialog } from '../../components/Overlay'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useModel } from '../../lib/observable'
import { useTr } from '../../lib/use-language'
import { useScreenViewed } from '../../store/analytics'
import { useStore } from '../../store/context'
import { createWorkflow, deleteWorkflow, fetchWorkflow, fetchWorkflowRun, renameWorkflow, restoreWorkflowVersion, retryWorkflowStep, runWorkflowStep, saveWorkflowGraph, saveWorkflowScript, setWorkflowEnabled, startWorkflowRun, stopWorkflowRun, stopWorkflowStep, decideWorkflowStep, workflowRuns, workflowVersion, workflowVersions } from './api'
import { UndoStack } from './history'
import { isWorkflowPinned, pinWorkflow } from './pins'
import { GraphSaver, type FlushResult } from './save'
import { WorkflowCanvas } from './WorkflowCanvas'
import { WorkflowHud } from './WorkflowHud'
import { cloneGraphNewIds, copyTitle, emptyGraph, latestStepByNode, type Graph, type RunDetail, type RunSummary, type WorkflowDetail, type WorkflowStep, type WorkflowVersion } from './workflow'
import './workflows.css'

type Phase = 'begin' | 'change' | 'end' | 'text'

export function WorkflowsView({ focusId }: { focusId?: string | null }) {
  const store = useStore()
  const tr = useTr()
  useModel(store)
  const [selected, setSelected] = useState<string | null>(focusId ?? null)
  const [undoable, setUndoable] = useState({ undo: false, redo: false })
  const [graph, setGraph] = useState<Graph>(emptyGraph())
  const [ready, setReady] = useState(false)
  const [notice, setNotice] = useState<string | null>(null)
  const [runs, setRuns] = useState<RunSummary[]>([])
  const [hasMore, setHasMore] = useState(false)
  const [liveRun, setLiveRun] = useState<RunDetail | null>(null)
  const [viewed, setViewed] = useState<RunDetail | null>(null)
  const [versions, setVersions] = useState<WorkflowVersion[]>([])
  const [versionGraph, setVersionGraph] = useState<Graph | null>(null)
  const [versionLabelText, setVersionLabelText] = useState<string | null>(null)
  const [solo, setSolo] = useState<WorkflowStep[]>([])
  const [renaming, setRenaming] = useState(false)
  const [name, setName] = useState('')
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [scriptOpen, setScriptOpen] = useState(false)
  const [scriptText, setScriptText] = useState('')
  const [source, setSource] = useState<'graph' | 'script'>('graph')
  const [now, setNow] = useState(() => Date.now())
  const [chrome, setChrome] = useState(true)
  const graphRef = useRef(graph)
  const dirtyRef = useRef(false)
  const hold = useRef(false)
  const history = useRef(new UndoStack<Graph>())
  const saver = useRef<GraphSaver | null>(null)
  const saveTimer = useRef<number | null>(null)
  const textTimer = useRef<number | null>(null)
  const textOpen = useRef(false)
  const inflight = useRef<Promise<FlushResult> | null>(null)
  const previewRef = useRef<WorkflowVersion | null>(null)
  const runIdRef = useRef<string | null>(null)
  const followRef = useRef(true)
  graphRef.current = graph

  const summary = store.workflows.find((item) => item.id === selected) ?? null
  const historical = viewed !== null && versionGraph === null
  const shown = versionGraph ?? (viewed && historical ? viewed.graph : graph)
  const steps = [...(historical ? viewed.steps : liveRun?.steps ?? []), ...solo]
  const live = !!liveRun && (liveRun.status === 'running' || liveRun.status === 'approval' || liveRun.status === 'stopping')
  const screen: Screen = !selected ? 'workflows' : versionGraph || historical ? 'workflowRun' : 'workflowEditor'
  useScreenViewed(screen, selected)

  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(timer)
  }, [])

  useEffect(() => { if (focusId) { hold.current = false; setSelected(focusId) } }, [focusId])

  useEffect(() => {
    if (hold.current || selected || !store.workflows.length) return
    setSelected(store.workflows[0]!.id)
  }, [store.workflows, selected])

  useEffect(() => {
    if (!store.workflowDeleted) return
    hold.current = true
    setSelected(null)
    setNotice(store.workflowDeleted)
    store.clearWorkflowDeleted()
  }, [store.workflowDeleted, store])

  const sync = (id: string | null, dirty: boolean) => {
    store.setWorkflowSession(id ? { id, dirty, runId: runIdRef.current, followLive: followRef.current } : null)
  }
  const noteHistory = () => setUndoable({ undo: history.current.canUndo, redo: history.current.canRedo })

  const adopt = (workflow: WorkflowDetail) => {
    const next = structuredClone(workflow.graph)
    graphRef.current = next
    setGraph(next)
    setReady(true)
    dirtyRef.current = false
    history.current.clear()
    noteHistory()
    setVersionGraph(null)
    setVersionLabelText(null)
    previewRef.current = null
    saver.current = new GraphSaver(workflow.revision, async (base, nextGraph) => saveWorkflowGraph(await store.ready(), workflow.id, base, nextGraph))
    setSource(workflow.source ?? 'graph')
    setScriptText(workflow.script ?? '')
    sync(workflow.id, false)
    store.upsertWorkflow(workflow)
    store.rememberWorkflowDetail(workflow)
  }

  useEffect(() => {
    if (!selected) {
      setReady(false)
      sync(null, false)
      return
    }
    let cancel = false
    setReady(false)
    setViewed(null)
    setSolo([])
    followRef.current = true
    void (async () => {
      try {
        const client = await store.ready()
        const { workflow } = await fetchWorkflow(client, selected, false)
        if (cancel) return
        adopt(workflow)
        const page = await workflowRuns(client, selected, 20)
        if (cancel) return
        setRuns(page.runs)
        setHasMore(page.hasMore)
        const open = page.runs.find((run) => run.status === 'running' || run.status === 'approval' || run.status === 'stopping')
        if (open) {
          const { run } = await fetchWorkflowRun(client, open.id)
          if (cancel) return
          runIdRef.current = run.id
          setLiveRun(run)
          sync(selected, dirtyRef.current)
        } else {
          runIdRef.current = null
          setLiveRun(null)
        }
      } catch (error) {
        if (!cancel && !store.reportWorkflowFailure(error)) setNotice(error instanceof Error ? error.message : String(error))
      }
    })()
    return () => { cancel = true }
  }, [selected])

  useEffect(() => {
    const detail = store.workflowDetail
    if (!detail || detail.id !== selected || dirtyRef.current) return
    adopt(detail)
  }, [store.workflowDetailToken])

  useEffect(() => {
    const run = store.workflowRun
    if (!run || !followRef.current || run.id !== runIdRef.current) return
    setLiveRun(run)
    setRuns((current) => upsertRun(current, run))
  }, [store.workflowRun])

  useEffect(() => () => store.setWorkflowSession(null), [store])

  const applyFlush = (result: FlushResult): 'ok' | 'failed' | 'conflict' => {
    if (result.status === 'conflict') {
      adopt(result.workflow)
      setNotice('This workflow was edited somewhere else')
      return 'conflict'
    }
    if (result.status === 'failed') {
      setNotice(result.error instanceof Error ? result.error.message : String(result.error))
      return 'failed'
    }
    if (result.status === 'saved') {
      dirtyRef.current = false
      sync(selected, false)
      if (result.workflow) store.upsertWorkflow(result.workflow)
    }
    return 'ok'
  }

  const settle = async (): Promise<'ok' | 'failed' | 'conflict'> => {
    const current = saver.current
    if (!current) return 'ok'
    if (saveTimer.current) window.clearTimeout(saveTimer.current)
    if (inflight.current) return applyFlush(await inflight.current)
    const pending = current.flush(graphRef.current)
    inflight.current = pending
    const result = await pending.finally(() => { if (inflight.current === pending) inflight.current = null })
    return applyFlush(result)
  }

  const schedule = () => {
    if (saveTimer.current) window.clearTimeout(saveTimer.current)
    saveTimer.current = window.setTimeout(() => void settle(), 400)
  }

  const edit = (next: Graph, phase: Phase) => {
    if (versionGraph || viewed) return
    if (phase === 'begin' || (phase === 'text' && !textOpen.current)) {
      history.current.push(structuredClone(graphRef.current))
      noteHistory()
    }
    if (phase === 'text') {
      textOpen.current = true
      if (textTimer.current) window.clearTimeout(textTimer.current)
      textTimer.current = window.setTimeout(() => { textOpen.current = false }, 400)
    }
    graphRef.current = next
    setGraph(next)
    dirtyRef.current = true
    saver.current?.mark(next)
    sync(selected, true)
    if (phase === 'end' || phase === 'text') schedule()
  }

  const restoreGraph = (next: Graph) => {
    graphRef.current = next
    setGraph(next)
    dirtyRef.current = true
    saver.current?.mark(next)
    sync(selected, true)
    schedule()
  }

  const undo = () => {
    const previous = history.current.undo(structuredClone(graphRef.current))
    if (previous) restoreGraph(previous)
    noteHistory()
  }
  const redo = () => {
    const next = history.current.redo(structuredClone(graphRef.current))
    if (next) restoreGraph(next)
    noteHistory()
  }

  const create = async () => {
    try {
      const { workflow } = await createWorkflow(await store.ready(), 'Workflow', emptyGraph())
      hold.current = false
      store.upsertWorkflow(workflow)
      setNotice(null)
      setSelected(workflow.id)
    } catch (error) {
      if (!store.reportWorkflowFailure(error)) setNotice(error instanceof Error ? error.message : String(error))
    }
  }

  const run = async (fromNodeId?: string) => {
    if (!selected || await settle() !== 'ok') return
    try {
      const { run: started } = await startWorkflowRun(await store.ready(), selected, crypto.randomUUID(), fromNodeId)
      runIdRef.current = started.id
      followRef.current = true
      setViewed(null)
      setLiveRun(started)
      setRuns((current) => upsertRun(current, started))
      sync(selected, dirtyRef.current)
    } catch (error) {
      if (!store.reportWorkflowFailure(error)) setNotice(error instanceof Error ? error.message : String(error))
    }
  }

  const open = (id: string) => { hold.current = false; setNotice(null); setSelected(id) }
  const pinned = selected ? isWorkflowPinned(store.computer.id, selected) : false
  const stepMap = latestStepByNode(steps)

  return (
    <div className="workflows">
      {chrome || !selected ? (
      <WorkflowHud
        title={summary?.title ?? ''}
        selectedId={selected}
        workflows={store.workflows}
        enabled={summary?.enabled ?? true}
        graph={ready ? shown : null}
        steps={[...stepMap.values()]}
        live={live}
        runs={runs}
        hasMore={hasMore}
        followLive={viewed === null}
        viewing={viewed}
        versions={versions}
        previewLabel={versionLabelText}
        pinned={pinned}
        notice={notice}
        scripted={source === 'script'}
        canUndo={undoable.undo}
        canRedo={undoable.redo}
        onUndo={undo}
        onRedo={redo}
        onOpen={open}
        onCreate={() => void create()}
        onRename={() => { setName(summary?.title ?? ''); setRenaming(true) }}
        onDuplicate={() => void duplicate()}
        onDelete={() => setConfirmDelete(true)}
        onPin={() => selected && pinWorkflow(store.computer.id, selected, !pinned)}
        onEnabled={(enabled) => void toggle(enabled)}
        onRun={() => void run()}
        onStop={() => void stop()}
        onLatest={() => { followRef.current = true; setViewed(null); sync(selected, dirtyRef.current) }}
        onRunOpen={(id) => void showRun(id)}
        onOlder={() => void older()}
        onLoadVersions={() => void loadVersions()}
        onPreview={(version) => void preview(version)}
        onRestore={() => void restore()}
        onScript={() => { setScriptText(scriptText); setScriptOpen(true) }}
        onShowHistory={() => void refreshRuns()}
      />
      ) : null}
      {selected && ready ? (
        <WorkflowCanvas
          workflowId={selected}
          graph={shown}
          readOnly={versionGraph !== null || historical}
          scripted={source === 'script'}
          ready={ready}
          steps={stepMap}
          now={now}
          onEdit={edit}
          onUndo={undo}
          onRedo={redo}
          onRun={() => void run()}
          onRunFrom={(nodeId) => void run(nodeId)}
          onRunStep={(nodeId) => void step(nodeId)}
          onRetry={(stepId) => void retry(stepId)}
          onDecide={(stepId, approved) => void decide(stepId, approved)}
          onStopStep={(stepId) => void haltStep(stepId)}
          onChrome={setChrome}
        />
      ) : selected ? <div className="workflow-viewport" /> : (
        <div className="workflow-empty">
          <VStack gap={8} align="center">
            {notice ? <Text tone="secondary">{notice}</Text> : null}
            {store.workflows.length === 0 ? <Text weight="semibold">No workflows yet</Text> : null}
            {store.workflows.length === 0 ? <Button onClick={() => void create()}>New workflow</Button> : null}
          </VStack>
        </div>
      )}
      <Dialog open={renaming} title="Rename" actions={[{ title: 'Rename', action: () => void rename() }]} onClose={() => setRenaming(false)}>
        <input className="workflow-input" data-autofocus aria-label="Name" value={name} onChange={(event) => setName(event.target.value)} />
      </Dialog>
      <Dialog open={confirmDelete} title="Delete workflow" actions={[{ title: 'Delete workflow', destructive: true, action: () => void remove() }]} onClose={() => setConfirmDelete(false)} />
      <Dialog open={scriptOpen} title={tr('Script')} actions={[{ title: 'Save', action: () => void saveScript() }]} onClose={() => setScriptOpen(false)}>
        <textarea className="workflow-input workflow-script" aria-label={tr('Script')} value={scriptText} onChange={(event) => setScriptText(event.target.value)} />
      </Dialog>
    </div>
  )

  async function saveScript() {
    if (!selected || !saver.current) return
    try {
      const result = await saveWorkflowScript(await store.ready(), selected, saver.current.revision, scriptText)
      if (!result.ok) {
        adopt(result.workflow)
        setNotice('This workflow was edited somewhere else')
      } else {
        adopt(result.workflow)
      }
      setScriptOpen(false)
    } catch (error) {
      if (!store.reportWorkflowFailure(error)) setNotice(error instanceof Error ? error.message : String(error))
    }
  }

  async function toggle(enabled: boolean) {
    if (!selected) return
    try {
      const { workflow } = await setWorkflowEnabled(await store.ready(), selected, enabled)
      store.upsertWorkflow(workflow)
    } catch (error) {
      if (!store.reportWorkflowFailure(error)) setNotice(error instanceof Error ? error.message : String(error))
    }
  }

  async function rename() {
    if (!selected) return
    setRenaming(false)
    const title = name.trim() || 'Workflow'
    try {
      const { workflow } = await renameWorkflow(await store.ready(), selected, title)
      store.upsertWorkflow(workflow)
    } catch (error) {
      if (!store.reportWorkflowFailure(error)) setNotice(error instanceof Error ? error.message : String(error))
    }
  }

  async function duplicate() {
    if (!selected || !summary) return
    try {
      const { workflow } = await createWorkflow(await store.ready(), copyTitle(summary.title), cloneGraphNewIds(graphRef.current))
      store.upsertWorkflow(workflow)
      open(workflow.id)
    } catch (error) {
      if (!store.reportWorkflowFailure(error)) setNotice(error instanceof Error ? error.message : String(error))
    }
  }

  async function remove() {
    if (!selected) return
    setConfirmDelete(false)
    const id = selected
    try {
      await deleteWorkflow(await store.ready(), id)
      store.removeWorkflow(id)
      hold.current = true
      setSelected(null)
    } catch (error) {
      if (!store.reportWorkflowFailure(error)) setNotice(error instanceof Error ? error.message : String(error))
    }
  }

  async function showRun(id: string) {
    try {
      const { run: detail } = await fetchWorkflowRun(await store.ready(), id)
      followRef.current = false
      setVersionGraph(null)
      setViewed(detail)
      sync(selected, dirtyRef.current)
    } catch (error) {
      if (!store.reportWorkflowFailure(error)) setNotice(error instanceof Error ? error.message : String(error))
    }
  }

  async function older() {
    const last = runs[runs.length - 1]
    if (!selected || !last) return
    const page = await workflowRuns(await store.ready(), selected, 20, { startedAt: last.startedAt, id: last.id })
    setRuns((current) => [...current, ...page.runs])
    setHasMore(page.hasMore)
  }

  async function loadVersions() {
    if (!selected) return
    const { versions: next } = await workflowVersions(await store.ready(), selected)
    setVersions(next)
  }

  async function preview(version: WorkflowVersion) {
    if (!selected) return
    const { graph: snapshot } = await workflowVersion(await store.ready(), selected, version.id)
    previewRef.current = version
    setViewed(null)
    setVersionGraph(snapshot)
    setVersionLabelText(tr('Version {revision} · Saved {time}').replaceAll('{revision}', String(version.revision)).replaceAll('{time}', new Date(version.createdAt).toLocaleString()))
  }

  async function restore() {
    const version = previewRef.current
    if (!selected || !version) return
    const { workflow } = await restoreWorkflowVersion(await store.ready(), selected, version.id)
    adopt(workflow)
    setNotice(null)
  }

  async function step(nodeId: string) {
    if (!selected || await settle() !== 'ok') return
    const { step: result } = await runWorkflowStep(await store.ready(), selected, nodeId, crypto.randomUUID())
    setSolo((current) => [...current, result])
  }

  async function retry(stepId: string) {
    const { step: result } = await retryWorkflowStep(await store.ready(), stepId)
    setSolo((current) => [...current, result])
  }

  async function decide(stepId: string, approved: boolean) {
    await decideWorkflowStep(await store.ready(), stepId, approved)
  }

  async function haltStep(stepId: string) {
    await stopWorkflowStep(await store.ready(), stepId)
  }

  async function stop() {
    if (!liveRun) return
    await stopWorkflowRun(await store.ready(), liveRun.id)
  }

  async function refreshRuns() {
    if (!selected) return
    const page = await workflowRuns(await store.ready(), selected, 20)
    setRuns((current) => {
      const byId = new Map(page.runs.map((run) => [run.id, run]))
      for (const run of current) if (!byId.has(run.id)) byId.set(run.id, run)
      return [...byId.values()].sort((a, b) => b.number - a.number)
    })
    setHasMore(page.hasMore)
  }
}

function upsertRun(current: RunSummary[], run: RunSummary): RunSummary[] {
  const next = current.filter((item) => item.id !== run.id)
  next.push({ id: run.id, number: run.number, source: run.source, status: run.status, startedAt: run.startedAt, endedAt: run.endedAt, error: run.error })
  return next.sort((a, b) => b.number - a.number)
}
