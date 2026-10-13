import { useEffect, useState } from 'react'
import type { UpdateState } from '@shared/ipc'
import { updateAction } from '@shared/update-action'
import { Button } from '../components/Controls'

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
