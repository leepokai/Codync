import { useState } from 'react'
import { Button } from '../components/Controls'
import { Icon } from '../components/Icon'
import { ModalHeader, Sheet } from '../components/Overlay'
import { font } from '../lib/fonts'
import { useModel } from '../lib/observable'
import { prefs, usePref } from '../lib/prefs'
import { afterStarAsk, dueStarAsk } from '../lib/star-ask'
import { useApp } from '../store/context'

const REPO = 'https://github.com/leepokai/Codync'

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
  const app = useApp()
  const local = useModel(app.local)
  const [onboarded] = usePref(prefs.onboardingDone)
  const [state, setState] = usePref(prefs.starAsk)
  const [closed, setClosed] = useState(false)
  const due = dueStarAsk(state, Date.now())
  const open = due !== null && onboarded && !closed && local?.connection.kind === 'online'

  const finish = (starred: boolean) => {
    if (closed) return
    setClosed(true)
    setState(afterStarAsk(state, starred, Date.now()))
    if (starred) window.codync.app.openExternal(REPO)
  }

  const text = copy[due ?? 'first']
  return (
    <Sheet open={open} onClose={() => finish(false)}>
      <div style={{ width: 420, display: 'flex', flexDirection: 'column' }}>
        <ModalHeader title={text.title} />
        <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 16, padding: '0 24px 24px' }}>
          <Icon name="star" size={36} weight="light" color="var(--secondary)" />
          <div style={{ ...font('callout'), color: 'var(--secondary)', textAlign: 'center' }}>{text.message}</div>
          <div style={{ display: 'flex', gap: 12 }}>
            <Button kind="secondary" onClick={() => finish(false)}>
              Not now
            </Button>
            <Button onClick={() => finish(true)}>Star on GitHub</Button>
          </div>
        </div>
      </div>
    </Sheet>
  )
}
