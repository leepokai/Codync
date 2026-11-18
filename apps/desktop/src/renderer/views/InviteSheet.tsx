import type { ReactElement } from 'react'
import appIcon from '../../../resources/icon.png'
import { Button } from '../components/Controls'
import { ModalHeader, Sheet } from '../components/Overlay'
import { HStack, VStack } from '../components/Stack'
import { Text } from '../components/Text'
import { useTr } from '../lib/use-language'
import './invite-sheet.css'

/**
 * An invitation over the chat window (GitHub star, Ko-fi): its symbol and title, a sentence, where it leads, then
 * *Not now* beside its action. Closing it any other way answers *Not now*.
 */
export function InviteSheet({ open, icon, title, message, url, action, onAnswer }: {
  open: boolean
  icon: ReactElement
  /** English phrases from the catalogs. */
  title: string
  message: string
  action: string
  /** Where it leads, shown without its scheme. */
  url: string
  onAnswer: (accepted: boolean) => void
}) {
  const tr = useTr()
  return (
    <Sheet open={open} onClose={() => onAnswer(false)} width={400} dismissOnDim>
      <VStack>
        <ModalHeader icon={icon} title={tr(title)} onClose={null} />
        <VStack gap={16} className="invite-sheet-body">
          <Text as="div" size="body" tone="secondary" className="invite-sheet-message">{tr(message)}</Text>
          <HStack gap={10}>
            <img src={appIcon} alt="" className="invite-sheet-app-icon" draggable={false} />
            <Text size="callout" tone="secondary">{url.replace(/^https?:\/\//, '')}</Text>
          </HStack>
          <HStack gap={8} justify="flex-end" wrap>
            <Button kind="secondary" onClick={() => onAnswer(false)}>
              Not now
            </Button>
            <Button onClick={() => onAnswer(true)}>
              {action}
            </Button>
          </HStack>
        </VStack>
      </VStack>
    </Sheet>
  )
}
