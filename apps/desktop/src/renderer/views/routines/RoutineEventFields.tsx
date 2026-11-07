import { CardSection, ChoicePicker } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import type { RoutineEventSource, RoutineScheduleDraft, RoutineSchedulePreview } from './routine'
import { LabeledRow, SchedulePreview } from './RoutineScheduleFields'
import './routines.css'

/**
 * An event trigger's fields: the service, the event it sends and its filters. The host keeps the
 * catalog and checks the result; the editor only fills the values.
 */
export function RoutineEventFields({ schedule, update, preview, error, sources }: {
  schedule: RoutineScheduleDraft
  update: (change: Partial<RoutineScheduleDraft>) => void
  /** The host's reading of exactly these fields, once it has checked them. */
  preview: RoutineSchedulePreview | null
  error: string | null
  sources: RoutineEventSource[]
}) {
  const tr = useTr()
  const source = sources.find((s) => s.id === schedule.eventSource) ?? sources[0]
  // A source the catalog doesn't list (an older trigger) is offered as it is, so saving it again
  // doesn't move its event to another service.
  const offers = source && !sources.some((s) => s.id === schedule.eventSource)
    ? [{ id: schedule.eventSource, label: schedule.eventSource, events: [{ id: schedule.eventName, label: schedule.eventName }], filters: [] }, ...sources]
    : sources
  if (!source) return <SchedulePreview preview={preview} error={error} />
  const pickSource = (id: string) => {
    const next = offers.find((s) => s.id === id)
    // A new service starts from its first event and has no filters of the old one.
    update({ eventSource: id, eventName: next?.events[0]?.id ?? '*', eventFilters: {} })
  }
  const setFilter = (key: string, value: string) => {
    const next = { ...schedule.eventFilters }
    if (value.trim()) next[key] = value
    else delete next[key]
    update({ eventFilters: next })
  }
  return (
    <CardSection footer={<SchedulePreview preview={preview} error={error} />}>
      <LabeledRow label={tr('Service')}>
        <ChoicePicker selection={source.id} options={offers.map((s) => ({ id: s.id, label: s.label }))} onChange={pickSource} fits />
      </LabeledRow>
      <LabeledRow label={tr('Event')}>
        <ChoicePicker
          selection={schedule.eventName}
          options={source.events.map((e) => ({ id: e.id, label: e.label }))}
          onChange={(eventName) => update({ eventName })}
          fits
        />
      </LabeledRow>
      {source.filters.map((filter) => (
        <LabeledRow key={filter.key} label={tr(filter.label)} top>
          <VStack gap={3} grow>
            <input
              className="schedule-input"
              aria-label={tr(filter.label)}
              value={schedule.eventFilters[filter.key] ?? ''}
              placeholder={filter.key === 'repo' ? 'owner/name' : ''}
              spellCheck={false}
              onChange={(event) => setFilter(filter.key, event.target.value)}
            />
            <HStack gap={6}>
              <Text size="caption" tone="tertiary">{tr(filter.hint)}</Text>
            </HStack>
          </VStack>
        </LabeledRow>
      ))}
    </CardSection>
  )
}
