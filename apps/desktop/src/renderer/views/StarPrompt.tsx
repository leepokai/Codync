import { useState } from 'react'
import appIcon from '../../../resources/icon.png'
import { Button } from '../components/Controls'
import { Icon } from '../components/Icon'
import { ModalHeader, Sheet } from '../components/Overlay'
import { HStack, VStack } from '../components/Stack'
import { Text } from '../components/Text'
import { useModel } from '../lib/observable'
import { prefs, usePref } from '../lib/prefs'
import { dueStarAsk } from '../lib/star-ask'
import { useStarAsk } from '../lib/use-star-ask'
import { useApp } from '../store/context'
import './star-prompt.css'

const copy = {
  first: {
    title: 'Star Codync on GitHub',
    message: 'Codync is free and open source. A star on GitHub is how other people find it.',
  },
  second: {
    title: 'Enjoying Codync?',
    message: "You've been using Codync for a while. If it saves you trips to your desk, a star on GitHub helps other people find it.",
  },
}

/**
 * Reminds once a week after the setup invitation; also covers older installations
 * that have not seen the setup invitation.
 * Starring ends it.
 */
export function StarPrompt() {
  const app = useApp()
  const local = useModel(app.local)
  const [onboarded] = usePref(prefs.onboardingDone)
  const { state, answer } = useStarAsk()
  const [closed, setClosed] = useState(false)
  const due = dueStarAsk(state, Date.now())
  const open = due !== null && onboarded && !closed && local?.connection.kind === 'online'

  const finish = (starred: boolean) => {
    if (closed || !due) return
    setClosed(true)
    answer(starred)
  }

  const text = copy[due ?? 'first']
  return (
    <Sheet open={open} onClose={() => finish(false)} width={400} dismissOnDim>
      <VStack className="star-prompt">
        <ModalHeader
          icon={<Icon name="star.fill" size={22} className="star-prompt-symbol" />}
          title={text.title}
          onClose={null}
        />
        <VStack gap={16} className="star-prompt-body">
          <Text as="div" size="body" tone="secondary" className="star-prompt-message">{text.message}</Text>
          <HStack gap={10}>
            <img src={appIcon} alt="" className="star-prompt-app-icon" draggable={false} />
            <Text size="callout" tone="secondary">github.com/leepokai/Codync</Text>
          </HStack>
          <HStack gap={8} justify="flex-end" wrap>
            <Button kind="secondary" onClick={() => finish(false)}>
              Not now
            </Button>
            <Button onClick={() => finish(true)}>
              Star on GitHub
            </Button>
          </HStack>
        </VStack>
      </VStack>
    </Sheet>
  )
}
