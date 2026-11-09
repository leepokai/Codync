import { useEffect, useState } from 'react'
import { Button, IconButton } from '../../components/Controls'
import { usePresence } from '../../components/Overlay'
import { HStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { KOFI_EXIT_MS, answerKofiInvite, noteKofiShown } from '../../lib/use-kofi-invite'
import { useTr } from '../../lib/use-language'
import './kofi-invite.css'

/** The Ko-fi invitation under the first reply this computer got (`useKofiInviteAt`): opening or dismissing it ends it. */
export function KofiInviteCard() {
  const tr = useTr()
  const [answered, setAnswered] = useState(false)
  const presence = usePresence(!answered, KOFI_EXIT_MS)
  useEffect(noteKofiShown, [])
  const answer = (opened: boolean) => {
    if (answered) return
    setAnswered(true)
    answerKofiInvite(opened)
  }
  if (!presence.mounted) return null
  return (
    <div className="chat-row notice">
      <HStack gap={10} className={`kofi-invite ${presence.shown ? 'shown' : ''}`}>
        <Text as="div" size="callout" tone="secondary" grow className="kofi-invite-text">
          {tr('Codync is free and made by one person. If it helps you, you can support it on Ko-fi.')}
        </Text>
        <Button kind="secondary" onClick={() => answer(true)}>Support on Ko-fi</Button>
        <IconButton title="Dismiss" icon="xmark" size={24} onClick={() => answer(false)} />
      </HStack>
    </div>
  )
}
