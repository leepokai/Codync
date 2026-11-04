import { useState } from 'react'
import type { Backend } from '@shared/models'
import { Icon } from '../../components/Icon'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { AgentSheet } from '../marketplace/AgentSheet'
import { FormSheet } from '../marketplace/pieces'
import './agent-status.css'

/**
 * The agent's sign-in as a compact status in the agent pickers: its account when signed in,
 * a reminder when installed but signed out, nothing when there is nothing to say (not
 * installed, or state unknown).
 */
export function AgentStatusBadge({ backend }: { backend: Backend }) {
  const tr = useTr()
  if (backend.signedIn === true) {
    return (
      <span className="agent-status fade-in" title={backend.email ?? tr('Signed in')}>
        <span aria-hidden className="agent-status-check"><Icon name="checkmark" size={8} weight="bold" color="var(--on-accent)" /></span>
        {backend.email ? <Text size="caption" tone="secondary" truncate>{backend.email}</Text> : null}
      </span>
    )
  }
  if (backend.installed === true && backend.signedIn === false) {
    return <span className="agent-status fade-in"><Text size="caption" tone="secondary">{tr('Not signed in')}</Text></span>
  }
  return null
}

/** The agent setup sheet (install, sign in), opened from the bot editor and the agent pickers. */
export function useAgentSetup() {
  const store = useStore()
  const [id, setId] = useState<string | null>(null)
  // Live: a sign-in updates the host's list while the sheet is open.
  const agent = id ? store.hello?.backends.find((b) => b.id === id) ?? null : null
  return {
    open: (backend: string) => setId(backend),
    sheet: (
      <FormSheet open={agent !== null} onClose={() => setId(null)} width={560} height={560} fixed>
        {agent ? <AgentSheet key={agent.id} initial={agent} /> : null}
      </FormSheet>
    ),
  }
}
