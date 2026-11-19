import { isGroup } from '@shared/models'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { planLimits, type PlanLimit } from './plan-limits'
import './plan-limits.css'

/**
 * Above the message box: the plan limits of the login the bot runs on, as a terminal's status line shows them.
 * The week carries its day and a mark where a steady pace would be. Nothing while the login has no reading.
 */
export function PlanLimits({ botId }: { botId: string }) {
  const store = useStore()
  const tr = useTr()
  const bot = store.bots.get(botId) ?? null
  if (!bot || isGroup(bot)) return null
  const limits = planLimits(store.usage, bot.backend, bot.loginProfile)
  if (!limits.length) return null
  return (
    <div className="plan-limits" role="group" aria-label={tr('Plan limits')}>
      {limits.slice(0, 3).map((l) => (
        <LimitItem key={l.id} limit={l} />
      ))}
    </div>
  )
}

function LimitItem({ limit }: { limit: PlanLimit }) {
  const tr = useTr()
  const pct = Math.min(100, Math.max(0, limit.percent))
  const detail = [
    `${tr(limit.title)} ${Math.round(limit.percent * 10) / 10}%`,
    limit.reset ? `${tr('resets in')} ${limit.reset}` : null,
    limit.day !== null ? `${tr('day')} ${limit.day}/7 · ${limit.pace}%` : null,
  ]
    .filter(Boolean)
    .join(' · ')
  return (
    <span className={`plan-limit ${limit.level}`} title={detail}>
      <span className="plan-limit-title">{tr(limit.title)}</span>
      <span className="plan-limit-value">{Math.round(limit.percent)}%</span>
      <span className="plan-limit-bar" aria-hidden="true">
        <span className="plan-limit-fill" style={{ width: `${pct}%` }} />
        {limit.pace !== null ? <span className="plan-limit-pace" style={{ left: `${limit.pace}%` }} /> : null}
      </span>
      {limit.reset ? <span className="plan-limit-reset">↻ {limit.reset}</span> : null}
      {limit.day !== null ? <span className="plan-limit-day">{limit.day}/7</span> : null}
    </span>
  )
}
