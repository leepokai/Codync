import { useEffect, useRef, useState } from 'react'
import { IconButton, Switch } from '../../components/Controls'
import { Sheet } from '../../components/Overlay'
import { HStack, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useScreenViewed } from '../../store/analytics'
import { useStore } from '../../store/context'
import { errorText, isActiveRun, runLabel, type Routine, type RoutineListing, type RoutineRun } from './routine'
import { RoutineEditorView } from './RoutineEditorView'
import './routines.css'
import { useTr } from '../../lib/use-language'


/** The open editor: on a routine, or (`routineId` null) setting up a new one. `id` is one per opening. */
interface EditorTarget {
  id: string
  routineId: string | null
}

/** The routine list in the details panel (and the routines sheet); a row opens its editor. */
export function RoutinesView({ botId, initialId, onClose }: { botId: string; initialId: string | null; onClose?: () => void }) {
  const tr = useTr()
  const store = useStore()
  const [routines, setRoutines] = useState<Routine[]>([])
  const [runs, setRuns] = useState<RoutineRun[]>([])
  const [editing, setEditing] = useState<EditorTarget | null>(null)
  const [loaded, setLoaded] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)
  const current = useRef(botId)
  current.current = botId

  const open = (routineId: string | null) => setEditing({ id: crypto.randomUUID(), routineId })
  // The list is part of the details panel; a routine's editor is the routines screen.
  useScreenViewed(editing ? 'routines' : null, editing?.id)

  const load = async () => {
    const asked = botId
    try {
      const result = await (await store.ready()).call<RoutineListing>('routines', { botId: asked })
      if (current.current !== asked) return
      setRoutines(result.routines)
      setRuns(result.runs)
      setLoaded(true)
      setLoadError(null)
    } catch (e) {
      if (current.current === asked) setLoadError(errorText(e))
    }
  }

  useEffect(() => {
    if (initialId) open(initialId)
  }, [initialId])

  useEffect(() => {
    setRoutines([])
    setRuns([])
    setLoaded(false)
    let stopped = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const tick = async () => {
      await load()
      if (!stopped) timer = setTimeout(tick, 3000)
    }
    void tick()
    return () => {
      stopped = true
      clearTimeout(timer)
    }
  }, [botId])

  const action = async (method: string, routine: Routine, enabled?: boolean) => {
    if (busy) return
    setBusy(true)
    try {
      await (await store.ready()).call(method, { botId, id: routine.id, enabled: enabled ?? null })
      setError(null)
      await load()
    } catch (e) {
      setError(errorText(e))
    } finally {
      setBusy(false)
    }
  }

  /** The row's second line: what's happening now, else when it runs. */
  const summary = (routine: Routine) => {
    const active = runs.find((r) => r.routineId === routine.id && isActiveRun(r))
    if (active) return runLabel(active)
    if (routine.lastError) return 'Schedule needs attention'
    const last = runs.find((r) => r.routineId === routine.id)
    if (last && ['failed', 'interrupted'].includes(last.status)) return `Last run ${runLabel(last).toLowerCase()}`
    return routine.triggerDescriptions.join(' · ')
  }

  const onSaved = (target: EditorTarget, saved: Routine | null) => {
    if (saved) setRoutines((list) => (list.some((r) => r.id === saved.id) ? list.map((r) => (r.id === saved.id ? saved : r)) : [...list, saved]))
    // A new webhook routine stays open: its URL and key exist only now.
    if (target.routineId === null && saved && saved.triggers.some((t) => t.type === 'webhook')) {
      setEditing({ id: target.id, routineId: saved.id })
    } else {
      setEditing(null)
    }
    void load()
  }

  const editingRoutine = routines.find((r) => r.id === editing?.routineId) ?? null
  return (
    <VStack gap={onClose ? 16 : 10} className="routines">
      <HStack>
        <Text size="body" weight="semibold" className="routines-title">{tr(`Routines`)}</Text>
        <Spacer />
        <IconButton title="Set up a routine" icon="plus" disabled={store.isOffline} onClick={() => open(null)} />
        {onClose ? <IconButton title="Close routines" icon="xmark" onClick={onClose} /> : null}
      </HStack>
      {loadError ? <Text size="caption" tone="danger">{loadError}</Text> : null}
      {error ? <Text size="caption" tone="danger" className="routines-error">{error}</Text> : null}
      {routines.length === 0 ? (
        <Text as="div" size="callout" tone="secondary" className="outlined-card routines-empty">
          {loaded ? tr(`No routines yet. Ask the bot for one, or set it up yourself with +.`) : tr(`Loading routines…`)}
        </Text>
      ) : (
        <VStack gap={8}>
          {routines.map((routine) => (
            <HStack key={routine.id} gap={12} className="outlined-card routines-card">
              <button className="routine-row" onClick={() => open(routine.id)}>
                <Text tone="text">{routine.name}</Text>
                <Text size="caption" tone={routine.lastError ? 'danger' : 'secondary'}>{summary(routine)}</Text>
              </button>
              <HStack as="span" align="stretch" aria-label={routine.name}>
                <Switch on={routine.enabled} disabled={busy || store.isOffline} onChange={(on) => void action('setRoutineEnabled', routine, on)} />
              </HStack>
            </HStack>
          ))}
        </VStack>
      )}
      {/* A routine opened before the list arrived waits for it. */}
      <Sheet open={editing !== null && (editing.routineId === null || editingRoutine !== null)} onClose={() => setEditing(null)} width={540}>
        {editing ? (
          <RoutineEditorView
            key={editing.id}
            botId={botId}
            routine={editingRoutine}
            saved={(saved) => onSaved(editing, saved)}
          />
        ) : null}
      </Sheet>
    </VStack>
  )
}
