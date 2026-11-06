import { useEffect, useRef, useState } from 'react'
import { Button, CardSection, IconButton, SegmentedChoice, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Dialog, ModalHeader } from '../../components/Overlay'
import { HStack, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useStore } from '../../store/context'
import { durationText, emptyDraft, errorText, repeatChoice, sameDraft, type Routine, type RoutineScheduleDraft, type RoutineSchedulePreview } from './routine'
import { Field } from '../bots/parts'
import { LabeledRow, RoutineScheduleFields } from './RoutineScheduleFields'
import { RoutineWebhookPanel } from './RoutineWebhookPanel'
import { currentZone } from './time-zones'
import './routines.css'
import { useTr } from '../../lib/use-language'


const saveShortcut = window.codync.platform === 'darwin' ? '⌘S' : 'Ctrl+S'

export function RoutineEditorView({ botId, routine, saved }: {
  botId: string
  routine: Routine | null
  /** Called after a save (the routine) or a delete (null). */
  saved: (routine: Routine | null) => void
}) {
  const tr = useTr()
  const store = useStore()
  const [name, setName] = useState(routine?.name ?? '')
  const [instruction, setInstruction] = useState(routine?.instruction ?? '')
  const [schedule, setSchedule] = useState<RoutineScheduleDraft>(emptyDraft)
  const [timeout, setTimeoutText] = useState(String(routine?.timeoutSeconds ?? 3600))
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [advanced, setAdvanced] = useState(false)
  const [loaded, setLoaded] = useState(false)
  const [preview, setPreview] = useState<RoutineSchedulePreview | null>(null)
  const [previewError, setPreviewError] = useState<string | null>(null)
  const [checkedSchedule, setCheckedSchedule] = useState<RoutineScheduleDraft | null>(null)
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [started, setStarted] = useState(false)
  const latest = useRef(schedule)
  latest.current = schedule

  // Anything the bot set up (a one-off time, an interval, events, several triggers) stays as it is until replaced.
  const keepsOriginal = routine ? (routine.triggers.length === 1 ? !['cron', 'webhook'].includes(routine.triggers[0]!.type) : routine.triggers.length > 0) : false
  const options = [
    ...(keepsOriginal ? [{ id: 'keep', label: 'Set by bot' }] : []),
    { id: 'cron', label: 'Schedule' },
    { id: 'webhook', label: 'Webhook' },
  ]
  const canSave = !!name.trim() && !!instruction.trim() && !store.isOffline && !busy && loaded && sameDraft(checkedSchedule, schedule) && previewError === null

  const loadSchedule = async () => {
    try {
      const client = await store.ready()
      const result = await client.call<RoutineSchedulePreview>('routineSchedule', {
        triggers: routine?.triggers ?? [],
        timeZone: currentZone(Intl.DateTimeFormat().resolvedOptions().timeZone),
      })
      const draft = { ...result.draft, calendarStyle: repeatChoice(result.draft.calendarStyle) }
      if (!['cron', 'webhook'].includes(draft.kind)) draft.kind = 'keep'
      setSchedule(draft)
      setPreview(result)
      setCheckedSchedule(result.draft)
      setLoaded(true)
      setError(null)
    } catch (e) {
      setError(errorText(e))
    }
  }

  const checkSchedule = async () => {
    const requested = latest.current
    setPreview(null)
    setPreviewError(null)
    setCheckedSchedule(null)
    try {
      const result = await (await store.ready()).call<RoutineSchedulePreview>('routineSchedule', { draft: requested })
      if (latest.current !== requested) return
      setPreview(result)
      setCheckedSchedule(requested)
    } catch (e) {
      if (latest.current !== requested) return
      setPreviewError(errorText(e))
    }
  }

  useEffect(() => {
    void loadSchedule()
  }, [])

  // The host checks each edit to the schedule, after a short pause in typing.
  useEffect(() => {
    if (!loaded) return
    const timer = setTimeout(() => void checkSchedule(), 300)
    return () => clearTimeout(timer)
  }, [schedule])

  const save = async () => {
    if (!canSave) return
    // Saving ends editing (a new webhook routine stays open on its URL and key).
    ;(document.activeElement as HTMLElement | null)?.blur()
    setBusy(true)
    setError(null)
    try {
      const client = await store.ready()
      const result = await client.call<{ routine: Routine }>('saveRoutine', {
        botId,
        id: routine?.id ?? null,
        name,
        instruction,
        schedule,
        timeoutSeconds: timeout,
      })
      saved(result.routine)
    } catch (e) {
      setError(errorText(e))
    } finally {
      setBusy(false)
    }
  }

  // ⌘S saves.
  const saveRef = useRef(save)
  saveRef.current = save
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 's') {
        e.preventDefault()
        void saveRef.current()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  const update = (change: Partial<RoutineScheduleDraft>) => setSchedule((s) => ({ ...s, ...change }))
  const shownPreview = sameDraft(checkedSchedule, schedule) ? preview : null

  const triggerFields = () => {
    switch (schedule.kind) {
      case 'cron':
        return <RoutineScheduleFields schedule={schedule} update={update} preview={shownPreview} error={previewError} />
      case 'webhook':
        return routine && routine.triggers.some((t) => t.type === 'webhook' || t.type === 'event') ? (
          <RoutineWebhookPanel botId={botId} routineId={routine.id} />
        ) : (
          <Note icon="link" title="Runs each time something is posted to its URL." detail="Save to get the URL and key." />
        )
      default:
        return <Note icon="sparkles" title={routine?.triggerDescriptions.join(' · ') ?? ''} detail="Set up by the bot. Pick Schedule or Webhook to replace it." />
    }
  }

  return (
    <VStack className="routine-editor">
      <ModalHeader icon={schedule.kind === 'webhook' ? 'link' : 'clock'} title={routine ? 'Edit routine' : 'Set up a routine'} />
      {/* The card fits the form; it scrolls only when taller than the window allows. */}
      <fieldset className="routine-form" disabled={busy || !loaded}>
        <VStack gap={24} className="routine-form-body">
          <VStack gap={16}>
            <Field label="Name">
              <input className="routine-input" value={name} placeholder="e.g. Morning summary" onChange={(e) => setName(e.target.value)} />
            </Field>
            <Field label="Instruction">
              <textarea className="routine-input" value={instruction} placeholder="Describe what this bot should do each time it runs." onChange={(e) => setInstruction(e.target.value)} />
            </Field>
          </VStack>
          <VStack gap={10}>
            <HStack gap={12}>
              <Text tone="text" grow>{tr(`When to run`)}</Text>
              <SegmentedChoice selection={schedule.kind} options={options} onChange={(kind) => update({ kind })} />
            </HStack>
            {loaded ? (
              triggerFields()
            ) : (
              <HStack as="span" gap={6} className="routine-loading">
                <Icon name="clock" size={12} />
                Loading schedule…
              </HStack>
            )}
          </VStack>
          <VStack gap={10}>
            <button className="press routine-advanced" aria-expanded={advanced} onClick={() => setAdvanced((a) => !a)}>
              Run settings
              <span className={`routine-chevron ${advanced ? 'open' : ''}`}>
                <Icon name="chevron.down" size={10} weight="semibold" />
              </span>
              <Spacer />
              {!advanced ? <Text tone="tertiary">Timeout {durationText(timeout)}</Text> : null}
            </button>
            {advanced ? (
              <div className="routine-fade">
                <CardSection footer="A run is stopped after this long. 1 second to 24 hours.">
                  <LabeledRow label="Timeout">
                    <input className="schedule-input routine-timeout" aria-label="Timeout in seconds" value={timeout} placeholder="3600" onChange={(e) => setTimeoutText(e.target.value)} />
                    <Text size="callout" tone="secondary">{tr(`seconds`)}</Text>
                  </LabeledRow>
                </CardSection>
              </div>
            ) : null}
          </VStack>
        </VStack>
      </fieldset>

      <VStack gap={12} className="routine-editor-footer">
        {previewError !== null ? (
          <button className="press routine-retry" disabled={store.isOffline} onClick={() => void checkSchedule()}>
            Check schedule again
          </button>
        ) : null}
        {!loaded && error !== null ? (
          <button className="press routine-retry" onClick={() => void loadSchedule()}>
            Retry loading schedule
          </button>
        ) : null}
        {schedule.kind !== 'cron' && previewError ? <Text size="caption" tone="danger">{previewError}</Text> : null}
        {error ? (
          <HStack as="span" gap={6} align="flex-start" className="routine-error">
            <Icon name="exclamationmark.circle" size={12} className="routine-error-icon" />
            {error}
          </HStack>
        ) : null}
        {store.isOffline ? (
          <HStack as="span" gap={6} className="routine-offline">
            <Icon name="wifi.slash" size={10} />
            Connect to this computer to save your routine.
          </HStack>
        ) : null}
        <HStack gap={4}>
          {routine ? (
            <>
              <IconButton title="Delete routine" icon="trash" onClick={() => setConfirmDelete(true)} />
              <IconButton
                title={started ? 'Started' : 'Test run'}
                icon={started ? 'checkmark' : 'play.circle'}
                disabled={started || store.isOffline}
                onClick={async () => {
                  try {
                    await (await store.ready()).call('runRoutine', { botId, id: routine.id })
                    setStarted(true)
                  } catch (e) {
                    setError(errorText(e))
                  }
                }}
              />
            </>
          ) : null}
          <Spacer />
          <Button disabled={!canSave} onClick={() => void save()} title={`Save (${saveShortcut})`}>
            {busy ? <Spinner size={12} /> : null}
            {busy ? 'Saving…' : routine ? 'Save changes' : 'Create routine'}
            {busy ? null : <span className="button-shortcut">{saveShortcut}</span>}
          </Button>
        </HStack>
      </VStack>

      <Dialog
        open={confirmDelete}
        title={tr('Delete routine?')}
        message={tr("This deletes the routine and stops its future runs. This can't be undone.")}
        actions={[
          {
            title: 'Delete routine',
            destructive: true,
            action: async () => {
              if (!routine) return
              try {
                await (await store.ready()).call('deleteRoutine', { botId, id: routine.id })
                saved(null)
              } catch (e) {
                setError(errorText(e))
              }
            },
          },
        ]}
        onClose={() => setConfirmDelete(false)}
      />
    </VStack>
  )
}

/** A trigger that has no fields here: what it does, and what to do about it. */
function Note({ icon, title, detail }: { icon: string; title: string; detail: string }) {
  return (
    <CardSection>
      <HStack gap={12} align="flex-start" className="routine-note">
        <Icon name={icon} size={13} color="var(--secondary)" className="routine-note-icon" />
        <VStack gap={3}>
          <Text size="callout" weight="medium" tone="text">{title}</Text>
          <Text size="callout" tone="secondary">{detail}</Text>
        </VStack>
      </HStack>
    </CardSection>
  )
}
