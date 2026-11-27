import { backgroundLabel, type WorkTask } from '@shared/work'
import { Spinner } from '../../components/Controls'
import { HStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { ComposerReveal } from './Composer'
import { Timer } from './WorkingIndicator'
import './work.css'

/**
 * What a thread's agent runs in the background (a workflow, a command, a monitor), in one capsule above the thread's
 * message box, as the bot's work is over its chat's: what the one piece does, or the names of several, and for how long.
 * Its task waits on that work, so the thread stays working between turns.
 */
export function BackgroundCapsule({ task }: { task: WorkTask }) {
  const tr = useTr()
  const label = backgroundLabel(task)
  return (
    <ComposerReveal open={label !== null}>
      {label ? (
        <div className="work-capsule-slot">
          <HStack gap={6} className="work-capsule plain" data-background-capsule>
            <Spinner size={12} />
            <Text size="callout" tone="secondary" className="work-capsule-lead">{tr('Background')} ·</Text>
            <Text size="callout" tone="text" truncate className="work-capsule-text">{label.text}</Text>
            <Timer since={label.since} />
          </HStack>
        </div>
      ) : null}
    </ComposerReveal>
  )
}
