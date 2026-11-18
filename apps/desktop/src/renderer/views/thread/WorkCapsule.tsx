import { useState } from 'react'
import { isGroup } from '@shared/models'
import { capsuleLabel } from '@shared/work'
import { IconButton, Spinner } from '../../components/Controls'
import { HStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { ComposerReveal } from './Composer'
import { StopWorkDialog, WorkDot } from './WorkDialogs'
import { Timer } from './WorkingIndicator'
import './work.css'

/**
 * The bot's work in the background, in one capsule above the composer while any item is live or waiting: that one
 * needs the user, how many work and wait, or what the one working item does and for how long. Its text opens the
 * bot's Details; Stop (one item with a task) asks once.
 */
export function WorkCapsule({ botId, open }: { botId: string; open: () => void }) {
  const tr = useTr()
  const store = useStore()
  const bot = store.bots.get(botId)
  const label = bot && !isGroup(bot) ? capsuleLabel(bot.work, tr) : null
  const [stopping, setStopping] = useState<string | null>(null)
  return (
    <>
      <ComposerReveal open={label !== null}>
        {label ? (
          <div className="work-capsule-slot">
            <HStack gap={2} className={label.stops ? 'work-capsule' : 'work-capsule plain'}>
              <button className="work-capsule-open" onClick={open}>
                {label.mark === 'working' ? <Spinner size={12} /> : <WorkDot mark={label.mark === 'needsYou' ? 'attention' : 'waiting'} />}
                <Text size="callout" tone="secondary" className="work-capsule-lead">{tr('In the background')} ·</Text>
                <Text size="callout" tone={label.mark === 'needsYou' ? 'attention' : 'text'} truncate>{label.text}</Text>
                {label.since !== null ? <Timer since={label.since} /> : null}
              </button>
              {label.stops ? <IconButton title="Stop" icon="stop.fill" size={24} onClick={() => setStopping(label.stops!.id)} /> : null}
            </HStack>
          </div>
        ) : null}
      </ComposerReveal>
      <StopWorkDialog botId={botId} itemId={stopping} onClose={() => setStopping(null)} />
    </>
  )
}
