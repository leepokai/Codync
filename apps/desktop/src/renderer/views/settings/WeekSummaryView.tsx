import { useEffect, useState } from 'react'
import type { WeekSummary } from '@shared/models'
import { CardSection, ValueRow } from '../../components/Controls'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import type { BotStore } from '../../store/bot-store'

/** The last seven days on one computer: what was said, done and approved. Counts only; no message text. */
export function WeekSummaryView({ store }: { store: BotStore }) {
  const tr = useTr()
  const [week, setWeek] = useState<WeekSummary | null>(null)
  useEffect(() => {
    let live = true
    void store.weekSummary().then((w) => live && setWeek(w))
    return () => { live = false }
  }, [store])
  if (!week || week.activeBots === 0) return null
  const rows: [string, string][] = [
    ['Messages you sent', String(week.messages)],
    ['Replies from your bots', String(week.replies)],
    ['Tool calls', String(week.tools)],
    ['File edits', String(week.fileEdits)],
    ['Approvals answered', `${week.approvalsAnswered} of ${week.approvalsAsked}`],
    ...(week.approvalsWaiting > 0 ? [['Approvals still waiting', String(week.approvalsWaiting)] as [string, string]] : []),
  ]
  return (
    <CardSection title={tr('Your week')} footer={week.topBots.length ? `${tr('Busiest')}: ${week.topBots.map((b) => b.name || tr('Bot')).join(', ')}` : null}>
      {rows.map(([label, value]) => (
        <ValueRow key={label} label={label}>
          <Text tone="text" tabular>{value}</Text>
        </ValueRow>
      ))}
    </CardSection>
  )
}
