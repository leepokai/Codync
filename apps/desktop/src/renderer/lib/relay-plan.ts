import type { CloudComputer } from '../store/cloud'

/**
 * Which account computers to open through the relay: approved ones (`access` `granted`) other than this computer
 * and any the app reaches otherwise (SSH). Of the relay stores `attached` earlier, those whose grant went are
 * closed, and those another route took over are just forgotten (`kept` is what stays attached).
 */
export function relayPlan(computers: CloudComputer[], localId: string | null, attached: ReadonlySet<string>, store: (id: string) => { isLoopback: boolean } | null) {
  const granted = new Map(computers.filter((c) => c.access === 'granted' && c.computerId !== localId).map((c) => [c.computerId, c]))
  const ours = [...attached].filter((id) => store(id)?.isLoopback === false)
  return {
    attach: [...granted.values()].filter((c) => !store(c.computerId)),
    detach: ours.filter((id) => !granted.has(id)),
    kept: new Set(ours.filter((id) => granted.has(id))),
  }
}
