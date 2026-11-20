import type { ReactNode } from 'react'
import type { Bot } from '@shared/models'
import { canRetry, taskLine, workSections } from '@shared/work'
import { IconButton } from '../../components/Controls'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { LiveWork, WorkRow, WorkSection } from './WorkRows'
import { Timer } from './WorkingIndicator'

/**
 * What the bot does, on its Details: **Threads** and **Event box** (`LiveWork`: what it works on now and what
 * waits), then `children` (what it does later and on a schedule), then **Recent** (tasks that ended in the last
 * day: Try again for one that didn't finish, unless another bot asked for it; Drop). A section with no task is left out. A row of a task a thread has
 * or had opens its thread in the inspector (`openThread`); one that needs the user opens the chat at its card (`openCard`).
 */
export function WorkTimeline({ bot, openCard, openThread, children }: { bot: Bot; openCard: (taskId: string) => void; openThread: (taskId: string) => void; children: ReactNode }) {
  const tr = useTr()
  const store = useStore()
  const { recent } = workSections(bot.work, bot.maxThreads)
  const offline = store.isOffline
  return (
    <>
      <LiveWork bot={bot} openCard={openCard} openThread={openThread} />
      {children}
      <WorkSection title="Recent" shown={recent.length > 0}>
        {recent.map((task) => (
          <WorkRow
            key={task.id}
            task={task}
            line={taskLine(task, tr)}
            open={task.thread ? () => openThread(task.id) : undefined}
            time={task.status === 'done' && task.startedAt !== null && task.endedAt !== null ? <Timer since={task.startedAt} until={task.endedAt} /> : null}
            always={!canRetry(task) ? null : <IconButton title="Try again" icon="arrow.clockwise" size={24} disabled={offline} onClick={() => store.retryWork(bot.id, task.id)} />}
            onHover={<IconButton title="Drop" icon="xmark" size={24} disabled={offline} onClick={() => store.dropWork(bot.id, task.id)} />}
          />
        ))}
      </WorkSection>
    </>
  )
}
