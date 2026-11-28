import { useState } from 'react'
import { useTr } from '../lib/use-language'
import { Icon } from './Icon'
import { Dialog } from './Overlay'

const leadAgentDescription = 'Your lead agent is your first point of contact. It follows your goals and unfinished tasks, suggests useful next steps, and coordinates work with your other bots. The lead Codync sets up has a Check in routine that runs twice a day by default; you can change or pause it in Routines.'

/**
 * The lead's crown. Decorative everywhere on desktop today: both call sites (the roster avatar and the
 * details panel) render it without `interactive`, so the role explanation below cannot be opened and the
 * `interactive` styles go unused. That is known and deferred — Android's details panel does open it
 * (`BotDetailsView.kt`), so the dialog is kept here rather than deleted, but desktop needs a decision on
 * where the explanation belongs before it is reachable again.
 */
export function LeadAgentBadge({ size, interactive = false }: { size: number; interactive?: boolean }) {
  const tr = useTr()
  const [open, setOpen] = useState(false)
  const crown = <Icon name="crown.fill" size={size * 0.78} color="var(--warning)" style={{ fill: 'var(--warning)' }} scaled={false} />
  const style = { width: size, height: size }
  if (!interactive) return <span className="lead-agent-badge" style={style} role="img" aria-label={tr('Lead agent')}>{crown}</span>
  return <>
    <button type="button" className="lead-agent-badge interactive" style={style} aria-label={tr('About the lead agent')}
      onClick={() => setOpen(true)}>{crown}</button>
    <Dialog open={open} title={tr('Lead agent')} message={tr(leadAgentDescription)} actions={[]} cancel="Done" onClose={() => setOpen(false)} />
  </>
}
