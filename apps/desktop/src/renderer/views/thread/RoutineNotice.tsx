import type { Entry } from '@shared/models'
import { IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'

/**
 * A routine's line in the chat, which opens the routine. A failed run that still offers a retry can
 * be tried again or ended there; the messages stay.
 */
export function RoutineNotice({ entry, open }: { entry: Entry; open: () => void }) {
  const tr = useTr()
  const store = useStore()
  const answering = store.answering.get(entry.id)
  const failed = entry.data.source === 'scheduled_task' && entry.data.style !== undefined
  return (
    <HStack gap={2} justify="center" align="center">
      <button className="chat-routine-notice" onClick={open}>
        <Icon name="clock.arrow.circlepath" size={10} />
        {tr(entry.data.text ?? 'Routine')}
      </button>
      {entry.data.retry &&
        (answering ? (
          <Spinner size={10} />
        ) : (
          <IconButton title={tr('Try again')} icon="arrow.clockwise" size={20} onClick={() => store.retryRoutineRun(entry)} />
        ))}
      {failed && !entry.data.cancelled && (
        <IconButton title={tr('Cancel task')} icon="xmark" size={20} onClick={() => store.cancelRoutineRun(entry)} />
      )}
      {entry.data.cancelled ? <Text size="caption" tone="tertiary">{tr('Cancelled')}</Text> : null}
    </HStack>
  )
}
