import { useEffect, useState } from 'react'
import { Button, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import type { BotStore } from '../../store/bot-store'

/** The Codync a computer runs, and its update when one is due (iOS `CodyncVersionRow`). */
export function CodyncVersionRow({ store }: { store: BotStore }) {
  const tr = useTr()
  // Moved on when an update asked from here runs out of time, so one that never finished is offered again.
  const [now, setNow] = useState(Date.now)
  const ends = store.hostUpdateWaitEnds
  useEffect(() => {
    if (ends === null) return
    const timer = setTimeout(() => setNow(Date.now()), Math.max(0, ends - Date.now()))
    return () => clearTimeout(timer)
  }, [ends])
  const line = store.hostVersionLine(Math.max(now, Date.now()))
  if (!line) return null
  return (
    <div className="settings-row settings-appear">
      <Icon name="arrow.down.circle" size={13} color="var(--secondary)" />
      <VStack grow className="settings-stack">
        <Text>Codync <Text tone="secondary" tabular>{line.version}</Text></Text>
        {line.detail.length > 0 ? (
          <Text size="caption" tone={line.warns ? 'attention' : 'secondary'}>{line.detail.map((sentence) => tr(sentence)).join(' ')}</Text>
        ) : null}
      </VStack>
      {line.accessory?.kind === 'updating' ? <Spinner />
        : line.accessory ? <Button onClick={() => store.requestHostUpdate()}>{line.accessory.title}</Button>
        : null}
    </div>
  )
}
