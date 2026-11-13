import type { CloudComputer } from '../store/cloud'

/**
 * The account computers this app opens through the relay: approved ones (`access` `granted`) other than this
 * computer and any the app reaches otherwise (SSH), whose store the relay doesn't own.
 */
export function relayComputers(computers: CloudComputer[], localId: string | null, owned: (id: string) => boolean) {
  return computers.filter((c) => c.access === 'granted' && c.computerId !== localId && owned(c.computerId))
}
