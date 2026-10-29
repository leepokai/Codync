import { useEffect, useState } from 'react'
import type { UpdateState } from '@shared/ipc'
import { updateAction } from '@shared/update-action'
import { Button, ProgressRing, Spinner } from '../components/Controls'
import { Icon } from '../components/Icon'

/** The app's update state, kept live. */
export function useUpdates() {
  const [updates, setUpdates] = useState<UpdateState | null>(null)
  useEffect(() => {
    void window.codync.updates.state().then(setUpdates)
    return window.codync.updates.onChange(setUpdates)
  }, [])
  return updates
}

/** Check, update, download progress, then Restart to update: whatever the update state offers. */
export function UpdateButton({ updates, kind = 'secondary' }: { updates: UpdateState; kind?: 'primary' | 'secondary' }) {
  const action = updateAction(updates)
  return (
    <Button kind={updates.staged ? 'primary' : kind} onClick={() => window.codync.updates.check()} disabled={!action.enabled}>
      {action.label}
    </Button>
  )
}

/**
 * The sidebar's Update: one tap downloads the release while Codync stays usable (a ring fills as
 * it downloads), then restarts into it.
 */
export function UpdateBadge({ size }: { size: number }) {
  const updates = useUpdates()
  if (!updates?.supported || !updates.availableVersion) return null
  const action = updateAction(updates)
  return (
    <button
      className="update-badge"
      style={{ width: size, height: size }}
      title={action.label}
      aria-label={action.label}
      aria-busy={updates.installing || updates.downloadPercent !== null}
      disabled={updates.installing}
      onClick={() => window.codync.updates.install()}
    >
      <BadgeIcon updates={updates} size={size} />
    </button>
  )
}

function BadgeIcon({ updates, size }: { updates: UpdateState; size: number }) {
  if (updates.installing) return <Spinner size={size * 0.5} />
  if (updates.downloadPercent !== null) return <ProgressRing value={updates.downloadPercent / 100} size={size * 0.5} />
  return <Icon name="arrow.down" size={size * 0.42} weight="semibold" />
}
