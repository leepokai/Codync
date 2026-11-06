import { useLayoutEffect, useRef } from 'react'
import type { Bot, Entry } from '@shared/models'
import { usePresence } from '../../components/Overlay'
import { WorkingIndicator } from './WorkingIndicator'
import { workingExitMs, workingTransition } from './working-transition'

/** Keep the activity and its space alive until the exit finishes, including a quick restart. */
export function WorkingTransition({ bot, visible, steps }: { bot: Bot; visible: boolean; steps: Entry[] }) {
  // Matches --working-duration; initial history appears without an entrance animation.
  const presence = usePresence(visible, workingExitMs, false)
  const lastActive = useRef(bot)
  const { active, indicator } = workingTransition(bot, lastActive.current, visible, presence.mounted)
  useLayoutEffect(() => { lastActive.current = active }, [active])
  return (
    <div className={`chat-working ${presence.shown ? 'shown' : ''}`} inert={!visible} aria-hidden={!visible}>
      <div className="chat-working-clip">
        <div className="chat-working-content">
          {/* Each turn starts folded. */}
          {indicator ? <WorkingIndicator key={indicator.startedAt ?? 0} bot={indicator} steps={steps} /> : null}
        </div>
      </div>
    </div>
  )
}
