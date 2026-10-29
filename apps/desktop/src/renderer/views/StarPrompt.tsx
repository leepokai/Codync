import { useState } from 'react'
import appIcon from '../../../resources/icon.png'
import { Button } from '../components/Controls'
import { Icon } from '../components/Icon'
import { ModalHeader, Sheet } from '../components/Overlay'
import { HStack, VStack } from '../components/Stack'
import { Text } from '../components/Text'
import { useModel } from '../lib/observable'
import { prefs, usePref } from '../lib/prefs'
import { afterStarAsk, dueStarAsk } from '../lib/star-ask'
import { useApp } from '../store/context'
import { analytics } from '../store/analytics'
import { useTr } from '../lib/use-language'


const [OWNER, NAME] = ['leepokai', 'Codync']
const REPO = `https://github.com/${OWNER}/${NAME}`

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
 * Asks for a GitHub star twice at most: once after setup, and once more a week later.
 * Starring ends it.
 */
export function StarPrompt() {
  const tr = useTr()
  const app = useApp()
  const local = useModel(app.local)
  const [onboarded] = usePref(prefs.onboardingDone)
  const [state, setState] = usePref(prefs.starAsk)
  const [closed, setClosed] = useState(false)
  const due = dueStarAsk(state, Date.now())
  const open = due !== null && onboarded && !closed && local?.connection.kind === 'online'

  const finish = (starred: boolean) => {
    if (closed || !due) return
    setClosed(true)
    setState(afterStarAsk(state, starred, Date.now()))
    // Covers the GitHub link too: it opens only on a star.
    analytics.track({ event: 'star_prompt_answered', ask: due, starred })
    if (starred) window.codync.app.openExternal(REPO)
  }

  const text = copy[due ?? 'first']
  return (
    <Sheet open={open} onClose={() => finish(false)}>
      <VStack className="star-prompt">
        <ModalHeader icon="star" title={text.title} />
        <VStack gap={16} className="star-prompt-body">
          <Text as="div" size="body" tone="secondary" className="star-prompt-message">{text.message}</Text>
          <button className="star-prompt-repo" title={REPO} onClick={() => finish(true)}>
            <img src={appIcon} alt="" className="star-prompt-icon" draggable={false} />
            <VStack gap={2} grow>
              <Text size="body" weight="semibold" tone="text" truncate>
                <Text weight="regular" tone="secondary">{OWNER} /</Text> {NAME}
              </Text>
              <Text size="callout" tone="secondary" truncate>{REPO.replace(tr(`https://`), '')}</Text>
            </VStack>
            <Icon name="arrow.up.right.square" size={13} color="var(--secondary)" />
          </button>
          <HStack gap={8} justify="flex-end">
            <Button kind="secondary" onClick={() => finish(false)}>
              Not now
            </Button>
            <Button className="star-prompt-star" onClick={() => finish(true)}>
              <Icon name="star" size={12} weight="semibold" />
              Star on GitHub
            </Button>
          </HStack>
        </VStack>
      </VStack>
    </Sheet>
  )
}
