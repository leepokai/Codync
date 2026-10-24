import type { Entry } from '@shared/models'
import { IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack } from '../../components/Stack'
import { useStore } from '../../store/context'

/** A routine's line in the chat, which opens the routine; a failed run the user can run again has a retry button. */
export function RoutineNotice({ entry, open }: { entry: Entry; open: () => void }) {
  const store = useStore()
  const retrying = store.answering.has(entry.id)
  return (
    <HStack gap={4} justify="center">
      <button className="chat-routine-notice" onClick={open}>
        <Icon name="clock.arrow.circlepath" size={10} />
        {entry.data.text ?? 'Routine'}
      </button>
      {entry.data.retry && (retrying ? <Spinner size={10} /> : <IconButton title="Try again" icon="arrow.clockwise" size={20} onClick={() => store.retryRoutineRun(entry)} />)}
    </HStack>
  )
}
