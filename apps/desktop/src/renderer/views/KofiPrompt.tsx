import { useEffect, useState } from 'react'
import { Icon } from '../components/Icon'
import { useModel } from '../lib/observable'
import { KOFI_URL, useKofiAsk } from '../lib/use-kofi-ask'
import { useApp } from '../store/context'
import { InviteSheet } from './InviteSheet'

/**
 * The Ko-fi invitation (`lib/kofi-ask.ts`), decided at each opening of the chat window: the launch, and each time it's
 * shown again after closing to the background (`windowOpened`), not when it only comes back from behind another one.
 */
export function KofiPrompt({ held }: { held: boolean }) {
  const [opening, setOpening] = useState(0)
  useEffect(() => window.codync.app.onCommand((c) => c.kind === 'windowOpened' && setOpening((n) => n + 1)), [])
  return <KofiVisit key={opening} held={held} />
}

function KofiVisit({ held }: { held: boolean }) {
  const local = useModel(useApp().local)
  const { open, answer } = useKofiAsk(held, local?.connection.kind === 'online')
  return (
    <InviteSheet
      open={open}
      icon={<Icon name="heart.fill" size={22} />}
      title="Support Codync"
      message="Codync is free and made by one person. If it helps you, you can support it on Ko-fi."
      url={KOFI_URL}
      action="Support on Ko-fi"
      onAnswer={answer}
    />
  )
}
