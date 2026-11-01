import { Switch } from '../../components/Controls'
import { HStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import type { Routine } from './routine'

/** One routine in a list: its name and a line under it (open the editor), and its on/off switch. */
export function RoutineRow({ routine, summary, disabled, onOpen, onToggle }: {
  routine: Routine
  summary: string
  disabled: boolean
  onOpen: () => void
  onToggle: (on: boolean) => void
}) {
  return (
    <HStack gap={12} className="outlined-card routines-card">
      <button className="routine-row" onClick={onOpen}>
        <Text tone="text">{routine.name}</Text>
        <Text size="caption" tone={routine.lastError ? 'danger' : 'secondary'}>{summary}</Text>
      </button>
      <HStack as="span" align="stretch" aria-label={routine.name}>
        <Switch on={routine.enabled} disabled={disabled} onChange={onToggle} />
      </HStack>
    </HStack>
  )
}
