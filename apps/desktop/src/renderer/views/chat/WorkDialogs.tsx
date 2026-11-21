import type { Bot } from '@shared/models'
import { givesWay, taskLine, type WorkTask, type WorkStatus } from '@shared/work'
import { Dialog } from '../../components/Overlay'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { Timer } from './WorkingIndicator'
import './work.css'

/** A task's mark: the attention dot, a moving dot, a faint ring, a ring, the danger dot. */
export type WorkMark = 'attention' | 'live' | 'waiting' | 'done' | 'danger'

export const WORK_MARK: Record<WorkStatus, WorkMark> = { needsYou: 'attention', working: 'live', waiting: 'waiting', done: 'done', failed: 'danger', stopped: 'danger' }

export function WorkDot({ mark }: { mark: WorkMark }) {
  return <span className={`work-dot ${mark}`} />
}

/** Asks once before stopping the working task `taskId`; closed while it is null. */
export function StopWorkDialog({ botId, taskId, onClose }: { botId: string; taskId: string | null; onClose: () => void }) {
  const tr = useTr()
  const store = useStore()
  return (
    <Dialog
      open={taskId !== null}
      title={tr('Stop this task?')}
      message={tr('What it already changed stays.')}
      actions={[{ title: 'Stop', action: () => taskId && store.stopWork(botId, taskId) }]}
      onClose={onClose}
    />
  )
}

/** Every thread is busy: asks which of the bot's live tasks that may give way does, to `task`; closed while it is null. */
export function CutInDialog({ bot, task, onClose }: { bot: Bot; task: WorkTask | null; onClose: () => void }) {
  const tr = useTr()
  const store = useStore()
  return (
    <Dialog
      open={task !== null}
      title={tr('Let "{title}" cut in').replaceAll('{title}', () => task?.title ?? '')}
      message={tr('Every thread is busy. Pick the task that gives way: its thread closes and the task starts again later.')}
      actions={[]}
      onClose={onClose}
    >
      <VStack gap={6}>
        {task ? bot.work.filter(givesWay).map((giving) => (
          <button
            key={giving.id}
            className="cut-in-pick"
            onClick={() => {
              store.workCutIn(bot.id, task.id, giving.id)
              onClose()
            }}
          >
            <WorkDot mark={WORK_MARK[giving.status]} />
            <VStack as="span" gap={2} align="flex-start" grow shrink>
              <Text tone="text" truncate>{giving.title}</Text>
              <Text size="caption" tone="secondary" truncate>{taskLine(giving, tr)}</Text>
            </VStack>
            {giving.startedAt !== null ? <Timer since={giving.startedAt} /> : null}
          </button>
        )) : null}
      </VStack>
    </Dialog>
  )
}
