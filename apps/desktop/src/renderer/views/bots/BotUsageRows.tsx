import { useEffect, useState } from 'react'
import { ValueRow } from '../../components/Controls'
import { useStore } from '../../store/context'
import { useTr } from '../../lib/use-language'

interface Usage {
  used: number
  limit: number | null
  fleetUsed: number
  fleetLimit: number | null
  resetsAt: number
}

const count = (tokens: number) => new Intl.NumberFormat().format(tokens)

/** Today's tokens for one bot, as the host's daily tally has them (with its limit, when it has one). */
export function BotUsageRows({ botId }: { botId: string }) {
  const store = useStore()
  const tr = useTr()
  const [usage, setUsage] = useState<Usage | null>(null)
  useEffect(() => {
    let live = true
    void (async () => {
      try {
        const answer = await (await store.ready()).call<Usage>('botUsage', { botId })
        if (live) setUsage(answer)
      } catch {
        // An older host without the tally leaves the rows out.
      }
    })()
    return () => { live = false }
  }, [botId, store])
  if (!usage) return null
  if (usage.used === 0 && usage.limit === null) return <ValueRow label={tr('Today')}>{tr('No tokens counted today')}</ValueRow>
  return (
    <>
      <ValueRow label={tr('Today')}>
        {usage.limit === null
          ? tr('{n} tokens').replace('{n}', count(usage.used))
          : tr('{used} of {limit} tokens').replace('{used}', count(usage.used)).replace('{limit}', count(usage.limit))}
      </ValueRow>
      {usage.fleetLimit !== null ? (
        <ValueRow label={tr('All bots today')}>{tr('{used} of {limit} tokens').replace('{used}', count(usage.fleetUsed)).replace('{limit}', count(usage.fleetLimit))}</ValueRow>
      ) : null}
    </>
  )
}
