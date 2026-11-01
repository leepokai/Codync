import { useEffect, useState } from 'react'
import { BotAvatar } from '../../components/Avatar'
import { IconButton } from '../../components/Controls'
import { ModalHeader, Sheet, SheetPage } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useScreenViewed } from '../../store/analytics'
import { useStore } from '../../store/context'
import { useTr } from '../../lib/use-language'
import { routineOwners, routineSections } from './all-routines'
import { errorText, type Routine } from './routine'
import { RoutineEditorView } from './RoutineEditorView'
import { RoutineRow } from './RoutineRow'
import './routines.css'

/** The open editor: a bot's routine, or (`routineId` null) a new one for that bot. `id` is one per opening. */
interface EditorTarget {
  id: string
  botId: string
  routineId: string | null
}

/** The account menu's Routines: every bot's routines, a section per bot, each with its switch; + sets one up for a bot. */
export function AllRoutinesView() {
  const tr = useTr()
  const store = useStore()
  const [routines, setRoutines] = useState<Routine[]>([])
  const [loaded, setLoaded] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [picking, setPicking] = useState(false)
  const [editing, setEditing] = useState<EditorTarget | null>(null)
  useScreenViewed(editing ? 'routines' : null, editing?.id)

  const bots = [...store.roster, ...store.hiddenBots]
  const owners = routineOwners(store.roster)

  const load = async () => {
    try {
      const result = await (await store.ready()).call<{ routines: Routine[] }>('allRoutines', {})
      setRoutines(result.routines)
      setLoaded(true)
      setError(null)
    } catch (e) {
      setError(errorText(e))
    }
  }

  useEffect(() => {
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
  }, [store])

  const toggle = async (routine: Routine, enabled: boolean) => {
    if (busy) return
    setBusy(true)
    try {
      await (await store.ready()).call('setRoutineEnabled', { botId: routine.botId, id: routine.id, enabled })
      await load()
    } catch (e) {
      setError(errorText(e))
    } finally {
      setBusy(false)
    }
  }

  const open = (botId: string, routineId: string | null) => {
    setPicking(false)
    setEditing({ id: crypto.randomUUID(), botId, routineId })
  }

  const onSaved = (target: EditorTarget, saved: Routine | null) => {
    // A new webhook routine stays open: its URL and key exist only now.
    if (target.routineId === null && saved && saved.triggers.some((t) => t.type === 'webhook')) {
      setRoutines((list) => [...list, saved])
      setEditing({ ...target, routineId: saved.id })
    } else {
      setEditing(null)
    }
    void load()
  }

  const summary = (routine: Routine) => (routine.lastError ? 'Schedule needs attention' : routine.triggerDescriptions.join(' · '))
  const sections = routineSections(routines, bots)
  const editingRoutine = routines.find((r) => r.id === editing?.routineId) ?? null
  return (
    <SheetPage>
      <ModalHeader
        icon="clock.arrow.circlepath"
        title={tr(picking ? `Which bot runs it?` : `Routines`)}
        trailing={picking ? (
          <IconButton title="Back" icon="chevron.left" onClick={() => setPicking(false)} />
        ) : (
          <IconButton title="Set up a routine" icon="plus" disabled={store.isOffline || owners.length === 0} onClick={() => (owners.length === 1 ? open(owners[0]!.id, null) : setPicking(true))} />
        )}
      />
      <VStack gap={16} className="routines all-routines">
        {error ? <Text size="caption" tone="danger" className="routines-error">{error}</Text> : null}
        {picking ? (
          <VStack gap={8}>
            {owners.map((bot) => (
              <button key={bot.id} className="outlined-card all-routines-owner" onClick={() => open(bot.id, null)}>
                <BotAvatar bot={bot} size={24} animated={false} />
                <Text tone="text" truncate>{bot.name}</Text>
              </button>
            ))}
          </VStack>
        ) : sections.length === 0 ? (
          <Text as="div" size="callout" tone="secondary" className="outlined-card routines-empty">
            {loaded ? tr(`No routines yet. Set one up for a bot with +.`) : tr(`Loading routines…`)}
          </Text>
        ) : (
          sections.map(({ bot, routines: own }) => (
            <VStack key={bot.id} gap={8}>
              <HStack gap={8} className="all-routines-bot">
                <BotAvatar bot={bot} size={20} animated={false} />
                <Text size="callout" weight="semibold" tone="secondary" truncate>{bot.name}</Text>
              </HStack>
              {own.map((routine) => (
                <RoutineRow
                  key={routine.id}
                  routine={routine}
                  summary={summary(routine)}
                  disabled={busy || store.isOffline}
                  onOpen={() => open(routine.botId, routine.id)}
                  onToggle={(on) => void toggle(routine, on)}
                />
              ))}
            </VStack>
          ))
        )}
      </VStack>
      {/* A routine opened before the list arrived waits for it. */}
      <Sheet open={editing !== null && (editing.routineId === null || editingRoutine !== null)} onClose={() => setEditing(null)} width={540}>
        {editing ? (
          <RoutineEditorView key={editing.id} botId={editing.botId} routine={editingRoutine} saved={(saved) => onSaved(editing, saved)} />
        ) : null}
      </Sheet>
    </SheetPage>
  )
}
