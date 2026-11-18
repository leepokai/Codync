import type { ReactNode } from 'react'
import type { Bot } from '@shared/models'
import { canRetry, itemLine, workSections } from '@shared/work'
import { IconButton } from '../../components/Controls'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { LiveWork, WorkRow, WorkSection } from './WorkRows'
import { Timer } from './WorkingIndicator'

/**
 * What the bot does, on its Details: **Threads** and **Event box** (`LiveWork`: what it works on now and what
 * waits), then `children` (what it does later and on a schedule), then **Recent** (items that ended in the last
 * day: Try again for one that didn't finish, unless another bot asked for it; Drop). A section with no item is left out. A row of an item a task has
 * or had opens its thread in the inspector (`openThread`); one that needs the user opens the chat at its card (`openCard`).
 */
export function WorkTimeline({ bot, openCard, openThread, children }: { bot: Bot; openCard: (itemId: string) => void; openThread: (itemId: string) => void; children: ReactNode }) {
  const tr = useTr()
  const store = useStore()
  const { recent } = workSections(bot.work, bot.maxAgents)
  const offline = store.isOffline
  return (
    <>
      <LiveWork bot={bot} openCard={openCard} openThread={openThread} />
      {children}
      <WorkSection title="Recent" shown={recent.length > 0}>
        {recent.map((item) => (
          <WorkRow
            key={item.id}
            item={item}
            line={itemLine(item, tr)}
            open={item.task ? () => openThread(item.id) : undefined}
            time={item.status === 'done' && item.startedAt !== null && item.endedAt !== null ? <Timer since={item.startedAt} until={item.endedAt} /> : null}
            always={!canRetry(item) ? null : <IconButton title="Try again" icon="arrow.clockwise" size={24} disabled={offline} onClick={() => store.retryWork(bot.id, item.id)} />}
            onHover={<IconButton title="Drop" icon="xmark" size={24} disabled={offline} onClick={() => store.dropWork(bot.id, item.id)} />}
          />
        ))}
      </WorkSection>
    </>
  )
}
