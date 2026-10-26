import { useState } from 'react'
import { useTr } from '../lib/use-language'
import { Icon } from './Icon'
import { Dialog } from './Overlay'

const leadAgentDescription = 'Your lead agent is your first point of contact. It follows your goals and unfinished tasks, suggests useful next steps, and coordinates work with your other bots. Its Check in routine runs twice a day by default; you can change or pause it in Routines.'

/** The roster badge is decorative; the larger details badge opens the role explanation. */
export function LeadAgentBadge({ size, interactive = false }: { size: number; interactive?: boolean }) {
  const tr = useTr()
  const [open, setOpen] = useState(false)
  const star = <Icon name="star.fill" size={size * 0.58} color="white" style={{ fill: 'white' }} scaled={false} />
  const style = { width: size, height: size }
  if (!interactive) return <span className="lead-agent-badge" style={style} role="img" aria-label={tr('Lead agent')}>{star}</span>
  return <>
    <button type="button" className="lead-agent-badge interactive" style={style} aria-label={tr('About the lead agent')}
      onClick={() => setOpen(true)}>{star}</button>
    <Dialog open={open} title={tr('Lead agent')} message={tr(leadAgentDescription)} actions={[]} cancel="Done" onClose={() => setOpen(false)} />
  </>
}
