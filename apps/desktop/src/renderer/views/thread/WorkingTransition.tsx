import { useLayoutEffect, useRef } from 'react'
import type { Bot } from '@shared/models'
import { usePresence } from '../../components/Overlay'
import { WorkingIndicator } from './ChatRows'
import { workingExitMs, workingTransition } from './working-transition'

/** Keep the activity and its space alive until the exit finishes, including a quick restart. */
export function WorkingTransition({ bot, visible }: { bot: Bot; visible: boolean }) {
  // Matches --working-duration; initial history appears without an entrance animation.
  const presence = usePresence(visible, workingExitMs, false)
  const lastActive = useRef(bot)
  const { active, indicator } = workingTransition(bot, lastActive.current, visible, presence.mounted)
  useLayoutEffect(() => { lastActive.current = active }, [active])
  return (
    <div className={`chat-working ${presence.shown ? 'shown' : ''}`} inert={!visible} aria-hidden={!visible}>
      <div className="chat-working-clip">
        <div className="chat-working-content">
          {indicator ? <WorkingIndicator bot={indicator} /> : null}
        </div>
      </div>
    </div>
  )
}
