import type { Bot } from '@shared/models'
import { givesWay, itemLine, type WorkItem, type WorkStatus } from '@shared/work'
import { Dialog } from '../../components/Overlay'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { Timer } from './WorkingIndicator'
import './work.css'

/** An item's mark: the attention dot, a moving dot, a faint ring, a ring, the danger dot. */
export type WorkMark = 'attention' | 'live' | 'waiting' | 'done' | 'danger'

export const WORK_MARK: Record<WorkStatus, WorkMark> = { needsYou: 'attention', working: 'live', waiting: 'waiting', done: 'done', failed: 'danger', stopped: 'danger' }

export function WorkDot({ mark }: { mark: WorkMark }) {
  return <span className={`work-dot ${mark}`} />
}

/** Asks once before stopping the working item `itemId`; closed while it is null. */
export function StopWorkDialog({ botId, itemId, onClose }: { botId: string; itemId: string | null; onClose: () => void }) {
  const tr = useTr()
  const store = useStore()
  return (
    <Dialog
      open={itemId !== null}
      title={tr('Stop the background work?')}
      message={tr('What it already changed stays.')}
      actions={[{ title: 'Stop', action: () => itemId && store.stopWork(botId, itemId) }]}
      onClose={onClose}
    />
  )
}

/** Every place is taken: asks which of the bot's live items that may give way does, to `item`; closed while it is null. */
export function CutInDialog({ bot, item, onClose }: { bot: Bot; item: WorkItem | null; onClose: () => void }) {
  const tr = useTr()
  const store = useStore()
  return (
    <Dialog
      open={item !== null}
      title={tr('Let "{title}" cut in').replaceAll('{title}', () => item?.title ?? '')}
      message={tr('Every place is taken. Pick what gives way: its session closes and it starts again later.')}
      actions={[]}
      onClose={onClose}
    >
      <VStack gap={6}>
        {item ? bot.work.filter(givesWay).map((giving) => (
          <button
            key={giving.id}
            className="cut-in-pick"
            onClick={() => {
              store.workCutIn(bot.id, item.id, giving.id)
              onClose()
            }}
          >
            <WorkDot mark={WORK_MARK[giving.status]} />
            <VStack as="span" gap={2} align="flex-start" grow shrink>
              <Text tone="text" truncate>{giving.title}</Text>
              <Text size="caption" tone="secondary" truncate>{itemLine(giving, tr)}</Text>
            </VStack>
            {giving.startedAt !== null ? <Timer since={giving.startedAt} /> : null}
          </button>
        )) : null}
      </VStack>
    </Dialog>
  )
}
