import type { UpdateState } from './ipc'

/** The one update action every surface offers (Settings, the update card, the menu bar). */
export function updateAction(u: UpdateState): { label: string; enabled: boolean } {
  if (u.installing) return { label: 'Restarting…', enabled: false }
  if (u.staged) return { label: 'Restart to update', enabled: true }
  if (u.downloadPercent !== null) return { label: `Downloading… ${u.downloadPercent}%`, enabled: false }
  if (u.checking) return { label: 'Checking…', enabled: false }
  return { label: u.availableVersion ? `Update to ${u.availableVersion}` : 'Check for updates', enabled: u.supported }
}
