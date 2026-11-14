import { useState } from 'react'
import { isGroup } from '@shared/models'
import { tasksSince, tasksWhat } from '@shared/tasks'
import { IconButton, Spinner } from '../../components/Controls'
import { Sheet } from '../../components/Overlay'
import { HStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { ComposerReveal } from './Composer'
import { StopTaskDialog, TasksView } from './TaskDetails'
import { Timer } from './WorkingIndicator'
import './tasks.css'

/**
 * The bot's background tasks, in one capsule above the composer while any is going: what they are doing and for how
 * long. Its text opens their details; Stop asks once.
 */
export function TaskCapsule({ botId }: { botId: string }) {
  const tr = useTr()
  const store = useStore()
  const bot = store.bots.get(botId)
  const tasks = bot && !isGroup(bot) ? bot.tasks : []
  const [showDetails, setShowDetails] = useState(false)
  const [stopping, setStopping] = useState(false)
  return (
    <>
      <ComposerReveal open={tasks.length > 0}>
        {tasks.length > 0 ? (
          <div className="task-capsule-slot">
            <HStack gap={2} className="task-capsule">
              <button className="task-capsule-open" onClick={() => setShowDetails(true)}>
                <Spinner size={12} />
                <Text size="callout" tone="secondary" className="task-capsule-lead">{tr('In the background')} ·</Text>
                <Text size="callout" tone="text" truncate>{tasksWhat(tasks, tr)}</Text>
                <Timer since={tasksSince(tasks)} />
              </button>
              <IconButton title="Stop" icon="stop.fill" size={24} onClick={() => setStopping(true)} />
            </HStack>
          </div>
        ) : null}
      </ComposerReveal>
      {/* One task is stopped by name; several, all at once. */}
      <StopTaskDialog botId={botId} taskId={tasks.length === 1 ? tasks[0]!.id : undefined} open={stopping} onClose={() => setStopping(false)} />
      <Sheet open={showDetails} onClose={() => setShowDetails(false)} width={620} height={560}>
        {showDetails ? <TasksView botId={botId} /> : null}
      </Sheet>
    </>
  )
}
