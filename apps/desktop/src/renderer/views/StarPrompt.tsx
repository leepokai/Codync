import { useState } from 'react'
import { Icon } from '../components/Icon'
import { useModel } from '../lib/observable'
import { prefs, usePref } from '../lib/prefs'
import { STAR_REPO, useStarAsk } from '../lib/use-star-ask'
import { useApp } from '../store/context'
import { InviteSheet } from './InviteSheet'

const copy = {
  first: {
    title: 'Star Codync on GitHub',
    message: 'Codync is free and open source. A star on GitHub is how other people find it.',
  },
  second: {
    title: 'Enjoying Codync?',
    message: 'If Codync saves you trips to your desk, a star on GitHub helps other people find it.',
  },
}

/**
 * Invites again at the fifth prompt after the setup invitation; also covers older
 * installations that have not seen the setup invitation.
 * Starring ends it.
 */
export function StarPrompt() {
  const app = useApp()
  const local = useModel(app.local)
  const [onboarded] = usePref(prefs.onboardingDone)
  const { due, prompts, answer } = useStarAsk()
  // The prompt count when this sheet last closed: the next ask waits for a later prompt, so two never follow each other.
  const [closedAt, setClosedAt] = useState(-1)
  const open = due !== null && onboarded && prompts > closedAt && local?.connection.kind === 'online'

  const finish = (starred: boolean) => {
    if (!open) return
    setClosedAt(prompts)
    answer(starred)
  }

  const text = copy[due ?? 'first']
  return (
    <InviteSheet
      open={open}
      icon={<Icon name="star.fill" size={22} className="invite-sheet-star" />}
      title={text.title}
      message={text.message}
      url={STAR_REPO}
      action="Star on GitHub"
      onAnswer={finish}
    />
  )
}
